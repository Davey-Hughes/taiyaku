//! `taiyaku library import|export`: the sidecar's documents, into and out of
//! the on-disk library `taiyaku serve` now reads and writes progress and
//! notes through.
//!
//! `import_canonical` (below) is also what `serve` (`main.rs`) calls, for
//! every sidecar path it is handed, before it builds the `Work`s it serves —
//! design §5's "`taiyaku serve <sidecar>...` keeps working. It imports those
//! paths and serves them."
//!
//! ## The hazard this module closes, and the decision it makes about it
//!
//! Task 13's `taiyaku-serve::works::row_id_for_write` mints a placeholder
//! `work` row so a reading position can be recorded before a sidecar is ever
//! imported. That row's `content_key` is the sentinel `bridge:{slug}` — never
//! a real 64-hex-character key — and its `sidecar_path` is whatever string
//! `taiyaku serve <path>` was invoked with, uncanonicalised.
//!
//! `taiyaku_library::import::sidecar` reconciles a later import against an
//! existing row by `content_key` first, which can never hit that sentinel,
//! and then by `sidecar_path` compared **as plain strings**. Two spellings of
//! one file — relative against absolute, a redundant `./`, a symlinked
//! parent — are the same work and different strings: an uncanonicalised
//! comparison misses, mints a *second* row, and silently orphans every note
//! and reading position recorded against the first. No error; the new work
//! simply looks empty.
//!
//! **Decision: canonicalise every sidecar path once, here, rather than
//! reconcile the placeholder by slug.** The two are not symmetric once
//! `taiyaku-serve`'s own slug derivation is in view: a placeholder row's slug
//! is `works::work_id(path)`, slugified from the sidecar's *filename*, while
//! `import::sidecar`'s `unique_slug` derives from the document's *title* —
//! two unrelated functions with no reason to ever agree, so "reconcile by
//! slug" would need a new correlation this schema does not carry (nothing on
//! a `work` row remembers which CLI-typed path minted it, only the path
//! itself, which is exactly the value in question). Canonicalising is a
//! single change at the one place a path enters this binary — `serve`'s
//! argument list and `library import`'s — needs no schema change, and
//! repairs the property the placeholder's own comparison already assumed
//! (that `sidecar_path` names one real file) rather than working around its
//! absence. Both `serve` and `library import` call through
//! [`import_canonical`], so a placeholder one mints and an import the other
//! runs still agree on what string names the file.
//!
//! Tested explicitly in this module's `tests.rs`: a placeholder row is
//! minted exactly as `row_id_for_write` mints one, carrying a reader's
//! recorded position, and a second import of the same file under a
//! different (but equivalent) spelling is shown to reuse it rather than
//! orphan it — and, discriminatingly, shown to orphan it when the same
//! import runs through `taiyaku_library::import::sidecar` directly, bypassing
//! this module's canonicalisation.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use taiyaku_core::{Merged, Sidecar};
use taiyaku_library::Library;

use crate::{flag_present, option_value, reject_unknown_flags};

/// Dispatch `library import|export`.
///
/// # Errors
///
/// Returns an error naming the subcommand if neither `import` nor `export`
/// was given, or whatever `import`/`export` itself returns.
pub(crate) fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("import") => import(&args[1..]),
        Some("export") => export(&args[1..]),
        Some(other) => Err(format!(
            "library has no subcommand `{other}`; it has import and export"
        )),
        None => Err("library needs a subcommand: import or export".to_string()),
    }
}

fn import(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library import", args, &["--db"])?;
    let positional: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    let [path] = positional.as_slice() else {
        return Err("usage: taiyaku library import <taiyaku.json> [--db PATH]".into());
    };
    let canonical = canonical_sidecar_path(Path::new(path))?;
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let (work_id, outcomes, migration) = import_canonical(&library, &canonical)?;
    for line in malformed_sibling_warnings(&canonical, &migration) {
        eprintln!("{line}");
    }
    println!(
        "imported {} as work {work_id}{}{}",
        canonical.display(),
        merge_summary(&outcomes),
        migration_summary(&migration)
    );
    Ok(())
}

fn export(args: &[String]) -> Result<(), String> {
    reject_unknown_flags(
        "library export",
        args,
        &["--all", "--db", "--dir", "--in-place"],
    )?;
    let dir = option_value(args, "--dir", "a directory")?.map(PathBuf::from);
    let in_place = flag_present(args, "--in-place");
    let all = flag_present(args, "--all");
    let positional: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    let usage = "usage: taiyaku library export <work>|--all [--dir DIR|--in-place] [--db PATH]";
    if all && !positional.is_empty() {
        return Err(format!("{usage}: --all takes no work name"));
    }
    if !all && positional.len() != 1 {
        return Err(format!("{usage}: name exactly one work, or pass --all"));
    }
    // C2: with neither flag, `export::work` defaults to writing back over
    // `sidecar_path` — the file a work was imported from. That is exactly
    // right when the database is the more current copy, but a hand
    // correction made through `taiyaku serve`'s alignment editor
    // (`routes.rs::patch_alignments`) reaches the sidecar file and the
    // in-memory `Work` only, never `state.library` — so between a
    // correction and the next `serve` start, the file can be ahead of the
    // database, and an unflagged export would silently revert it. Refusing
    // here, before any database is opened, makes overwriting the source a
    // decision a caller states rather than a default nobody chose.
    if dir.is_none() && !in_place {
        return Err(format!(
            "{usage}: refusing to write without --dir or --in-place — with neither, export \
             would write back over the sidecar each work was imported from, silently reverting \
             any change made to that file since. Pass --in-place to accept that, or --dir DIR \
             to write elsewhere instead"
        ));
    }
    // `taiyaku_read::atomic::write` stages its temporary beside the target,
    // so the target directory has to exist before the first document is
    // written; a bare `--dir` naming a directory a caller has not created
    // yet is the ordinary case, not a mistake, so this creates it rather
    // than refusing. `export::work` creates the per-work subdirectory
    // underneath it — `DIR/<slug>/` — which is what stops every work in a
    // library, all of whose sidecars are named `taiyaku.json`, from writing
    // to the same five paths and overwriting each other (review 2's C1).
    // Creating the parent here as well keeps an unusable `--dir` (a typo
    // into an unwritable place) a refusal before any database is opened.
    if let Some(d) = &dir {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }

    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let conn = library.conn();
    let targets: Vec<(i64, String)> = if all {
        all_work_ids(&conn)?
    } else {
        vec![work_by_slug(&conn, positional[0])?]
    };

    export_targets(&conn, &targets, dir.as_deref()).map(|_| ())
}

/// Export every named work, and report what happened to each.
///
/// **A work that fails does not abort the ones after it.** This loop used to
/// propagate with `?`, so under `--all` a single bad work — a NULL
/// `sidecar_path`, which `clear_stale_path_claim` can produce, most plausibly
/// — left the works before it written and the works after it neither written
/// nor named (review 2, Minor 1). Under `--in-place` that is the worst
/// available outcome: some of a reader's sources overwritten from the
/// database, the rest not, and no list of which.
///
/// The alternative considered and rejected was refusing before writing
/// anything. It cannot be done honestly: the failures here include a
/// serialisation error, a directory that cannot be created and a full disk,
/// none of which is knowable without doing the work, so a pre-flight could
/// only check the one predictable case and would sell an atomicity it does not
/// have. `export::work` already says so about the five files of one work — "a
/// document that did write is never wrong for having written, only possibly
/// out of step with a sibling that did not" — and this is the same claim one
/// level up. It is also the trade `serve_works` makes on the way in: one
/// sidecar that cannot be imported is skipped and named, not fatal.
///
/// So: write what can be written, name every failure on stderr as it happens,
/// and still exit non-zero, with a final line saying how many of how many
/// failed so a script sees it and a person can act on the names above. The
/// stdout summary counts the works that actually exported, not the works
/// asked for — "exported 12 document(s) across 3 work(s)" on a four-work
/// `--all` is the count that matches the files.
///
/// **Returns that document count**, which is the only reason this is not a
/// `Result<(), String>`. The figure lived in a `println!` and nowhere else, so
/// nothing could assert it: `+=` → `*=` left every summary reading "exported 0
/// document(s)" and passed the suite, since no `--all` fixture had enough
/// works and documents to tell the two apart (whole-branch review 3, Minor
/// 11). Its one production caller drops it — the command's output is the
/// printed line — but a claim a person reads off the terminal after
/// overwriting their own library is worth a test, and a test needs a value.
fn export_targets(
    conn: &rusqlite::Connection,
    targets: &[(i64, String)],
    dir: Option<&Path>,
) -> Result<usize, String> {
    let mut total = 0usize;
    let mut failed: Vec<&str> = Vec::new();
    for (id, slug) in targets {
        match taiyaku_library::export::work(conn, *id, dir) {
            Ok(exported) => {
                for path in &exported.written {
                    println!("wrote {}", path.display());
                }
                // Named as loudly as a write, and on stdout beside them: under
                // `--in-place` this deleted a file in the reader's own library
                // because their state for it is now empty — see
                // `export::work`'s doc comment for why a stale document is
                // removed rather than left.
                for path in &exported.removed {
                    println!("removed {} (no longer any state for it)", path.display());
                }
                // The third outcome, and the one a person has to be able to
                // act on: a sibling document this library has never had the
                // content of, so neither overwritten nor taken away. See
                // `export::work`'s "What an export will not touch" for the
                // three ways a file gets into that state and the one way out.
                //
                // **"a fresh one" was a promise this cannot keep** (delta
                // review M8). A document with no state behind it is not
                // written at all — the rule `export::work` applies to all of
                // them and `taiyaku.library.json` joined on this branch — and
                // an empty database side is exactly the state that produces a
                // `kept` in the first place. So moving the file aside can
                // leave the reader with nothing where the sentence had
                // promised a skeleton to fill in. What it can honestly say is
                // what the export would put there, including when that is
                // nothing.
                for path in &exported.kept {
                    println!(
                        "kept {} (this library has never read it, so it was not \
                         overwritten; move it aside and export again to replace it with \
                         what this library holds — which is no file at all where it \
                         holds nothing)",
                        path.display()
                    );
                }
                total += exported.written.len();
            }
            Err(e) => {
                eprintln!("taiyaku: exporting `{slug}`: {e}");
                failed.push(slug);
            }
        }
    }
    println!(
        "exported {total} document(s) across {} work(s)",
        targets.len() - failed.len()
    );
    if failed.is_empty() {
        return Ok(total);
    }
    // Phrased to read correctly at both ends of the range: `--all` over a
    // hundred works with one bad row, and a single named work that failed,
    // where there is no "rest" for a claim about it to be true of.
    Err(format!(
        "{} of {} work(s) could not be exported: {}. The reason for each is on \
         stderr above; what could be written, was",
        failed.len(),
        targets.len(),
        failed.join(", ")
    ))
}

fn all_work_ids(conn: &rusqlite::Connection) -> Result<Vec<(i64, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT id, slug FROM work ORDER BY id")
        .map_err(|e| format!("listing works: {e}"))?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("listing works: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading works: {e}"))
}

fn work_by_slug(conn: &rusqlite::Connection, slug: &str) -> Result<(i64, String), String> {
    conn.query_row("SELECT id, slug FROM work WHERE slug = ?1", [slug], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
    .map_err(|e| format!("looking up work `{slug}`: {e}"))?
    .ok_or_else(|| format!("no work `{slug}` in this library"))
}

/// Fixed-field style, the discipline `--stats` (`stats.rs`) and
/// `duplicate-survey.sh` established: a merge's outcomes are counted rather
/// than swallowed, per spec §4.2. Empty for a work the library had never
/// seen before — `reimport` returns no `Merged` outcomes for a plain import,
/// since there is nothing held to have merged into.
///
/// `dropped_hand_runs` is C1's field, added rather than reported through a
/// second format: `reimport` never lets a `merge_spoke` refusal fail the
/// whole import (`serve` must never fail to start over a stale alignment),
/// but the hand-corrected runs a dropped spoke held are a human decision, not
/// a tool's output, so — like `carried_hand_runs` and unlike
/// `dropped_credits` — the count is always shown here rather than only where
/// non-zero.
fn merge_summary(outcomes: &[Merged]) -> String {
    if outcomes.is_empty() {
        return String::new();
    }
    let added = outcomes
        .iter()
        .filter(|m| matches!(m, Merged::Added))
        .count();
    let replaced = outcomes
        .iter()
        .filter(|m| matches!(m, Merged::Replaced))
        .count();
    let carried: usize = outcomes
        .iter()
        .map(|m| match m {
            Merged::CarriedHandRuns(n) => *n,
            _ => 0,
        })
        .sum();
    let dropped: usize = outcomes
        .iter()
        .map(|m| match m {
            Merged::DroppedCredits(n) => *n,
            _ => 0,
        })
        .sum();
    let dropped_hand_runs: usize = outcomes
        .iter()
        .map(|m| match m {
            Merged::DroppedHandRuns(n) => *n,
            _ => 0,
        })
        .sum();
    format!(
        " (added={added} replaced={replaced} carried_hand_runs={carried} dropped_credits={dropped} dropped_hand_runs={dropped_hand_runs})"
    )
}

/// `merge_summary`'s own fixed-field discipline, for what `import_canonical`'s
/// new call to `taiyaku_library::migrate::work` read back from a work's up to
/// four sibling documents on its first import — the whole-branch review's
/// "C3" defect, `.superpowers/sdd/lib-fix-c3-context.md`, not to be confused
/// with the roadmap's spec C3 (`librarydoc.rs`'s metadata-editing screen,
/// still unbuilt). A document with no sibling file
/// beside the sidecar at all is omitted entirely — the same way
/// `merge_summary` prints nothing for a plain import with nothing to report —
/// so a library that has never used the sibling-file convention stays silent
/// rather than printing four `=absent` fields on every import forever.
///
/// The token behind a document that *was* migrated is `migrated`, plus a
/// count where one means something (`notes`'s note count, `truth`'s pages and
/// boxes). `kept` means the database already held state for that document and
/// the file was left unread, never overwritten (the one-way-migration rule).
/// `unread` means the same file was left unread **and the database holds
/// nothing for it either** — the state to look at twice, since neither side
/// has that document: either it was read once and its state has since been
/// cleared, or it appeared after the first import and nothing here has ever
/// read it. `malformed` means the file existed but could not be read, parsed
/// or applied; the reason is not embedded here, since a fixed-field line is
/// no place for a sentence with spaces in it — see
/// `malformed_sibling_warnings`, which gives the reason in full.
///
/// **`kept` and `unread` were one token until review 3's C1**, and it was the
/// wrong one: `kept` was printed for every already-settled document, so a
/// reader whose `taiyaku.truth.json` had been copied back from another
/// machine was told the database already held their labelling — in the line
/// immediately before the export that deleted the file, with the database
/// holding nothing.
pub(crate) fn migration_summary(report: &taiyaku_library::migrate::SiblingMigration) -> String {
    use taiyaku_library::migrate::{LibraryOutcome, NotesOutcome, ProgressOutcome, TruthOutcome};

    if report.is_empty() {
        return String::new();
    }

    let mut fields = Vec::new();
    if let Some(outcome) = &report.progress {
        fields.push(format!(
            "progress={}",
            match outcome {
                ProgressOutcome::Applied => "migrated".to_string(),
                ProgressOutcome::AlreadyPresent => "kept".to_string(),
                ProgressOutcome::Unread => "unread".to_string(),
                ProgressOutcome::Malformed(_) => "malformed".to_string(),
            }
        ));
    }
    if let Some(outcome) = &report.notes {
        fields.push(format!(
            "notes={}",
            match outcome {
                NotesOutcome::Applied { count } => format!("migrated:{count}"),
                NotesOutcome::AlreadyPresent => "kept".to_string(),
                NotesOutcome::Unread => "unread".to_string(),
                NotesOutcome::Malformed(_) => "malformed".to_string(),
            }
        ));
    }
    if let Some(outcome) = &report.truth {
        fields.push(format!(
            "truth={}",
            match outcome {
                TruthOutcome::Applied { pages, boxes } => format!("migrated:{pages}p/{boxes}b"),
                TruthOutcome::AlreadyPresent => "kept".to_string(),
                TruthOutcome::Unread => "unread".to_string(),
                TruthOutcome::Malformed(_) => "malformed".to_string(),
            }
        ));
    }
    if let Some(outcome) = &report.library {
        fields.push(format!(
            "library={}",
            match outcome {
                LibraryOutcome::Applied => "migrated".to_string(),
                LibraryOutcome::AlreadyPresent => "kept".to_string(),
                LibraryOutcome::Unread => "unread".to_string(),
                LibraryOutcome::Malformed(_) => "malformed".to_string(),
            }
        ));
    }
    format!(" (siblings: {})", fields.join(" "))
}

/// The sentence behind every `=malformed` token `migration_summary` prints —
/// one per document that could not be read, in the order `migrate::work`
/// considers them, and empty when nothing was malformed. Its caller writes
/// them to stderr in `resolve_db_path`'s own `"taiyaku: ..."` form, since a bad
/// sibling file is real information for whoever runs this and a fixed-field
/// stdout line is not where a message with a colon and a byte offset in it
/// belongs. Never fails: this is purely reporting a decision
/// `taiyaku_library::migrate::work` already made, to skip that one document
/// rather than fail the whole import over it.
///
/// **Returns the lines rather than printing them, which is the whole reason
/// this function changed shape** (whole-branch review 3, Minor 10). It printed
/// directly, so replacing its body with `()` passed the entire suite: the only
/// message that tells a reader a sibling document was not read was pinned by
/// nothing. That matters most for exactly the file the C1 fix protects — a
/// malformed `taiyaku.truth.json` is left on disk *so that* fixing it and
/// importing again works, and this line is how a reader learns there is
/// something to fix. `migration_summary` above already had this shape for the
/// same reason; the two now match.
pub(crate) fn malformed_sibling_warnings(
    canonical: &Path,
    report: &taiyaku_library::migrate::SiblingMigration,
) -> Vec<String> {
    use taiyaku_library::migrate::{LibraryOutcome, NotesOutcome, ProgressOutcome, TruthOutcome};

    let path = canonical.display();
    let mut lines = Vec::new();
    if let Some(ProgressOutcome::Malformed(msg)) = &report.progress {
        lines.push(format!("taiyaku: {path}: progress not migrated: {msg}"));
    }
    if let Some(NotesOutcome::Malformed(msg)) = &report.notes {
        lines.push(format!("taiyaku: {path}: notes not migrated: {msg}"));
    }
    if let Some(TruthOutcome::Malformed(msg)) = &report.truth {
        lines.push(format!("taiyaku: {path}: truth not migrated: {msg}"));
    }
    if let Some(LibraryOutcome::Malformed(msg)) = &report.library {
        lines.push(format!(
            "taiyaku: {path}: library metadata not migrated: {msg}"
        ));
    }
    lines
}

/// Where a sidecar path becomes a database identity — see this module's own
/// doc comment for the hazard this exists to close.
///
/// # Errors
///
/// Returns an error, naming `path` exactly as typed, if it cannot be
/// canonicalised: `std::fs::canonicalize` needs the file (and every parent
/// directory named in a symlink) to exist, so a typo is reported here rather
/// than surfacing later as a stranger error out of SQLite or
/// `Sidecar::from_json`.
pub(crate) fn canonical_sidecar_path(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Import one sidecar whose path is **already canonical** — see
/// `canonical_sidecar_path`, the one function both `import` above and `serve`
/// (`main.rs`) call before this, so a work bridged by one command and
/// reconciled by the other agree on what string names it.
///
/// # Errors
///
/// Returns an error if `path` cannot be read or parsed as a sidecar, or if
/// `taiyaku_library::import::reimport` fails. A malformed *sibling* document
/// (`.progress.json`, `.notes.json`, `.truth.json`, `.library.json`) never
/// reaches this `Err` — see `taiyaku_library::migrate::work`'s own doc
/// comment — it is folded into the returned `SiblingMigration` instead.
pub(crate) fn import_canonical(
    library: &Library,
    canonical: &Path,
) -> Result<(i64, Vec<Merged>, taiyaku_library::migrate::SiblingMigration), String> {
    let text =
        std::fs::read_to_string(canonical).map_err(|e| format!("{}: {e}", canonical.display()))?;
    let doc = Sidecar::from_json(&text)?;
    let conn = library.conn();
    let (work_id, outcomes) = taiyaku_library::import::reimport(&conn, &doc, canonical)?;
    let migration = taiyaku_library::migrate::work(&conn, work_id, canonical)?;
    Ok((work_id, outcomes, migration))
}

/// `--db PATH`, or `$XDG_DATA_HOME/taiyaku/library.db`, or
/// `$HOME/.local/share/taiyaku/library.db` — pure in its inputs, the same
/// discipline `taiyaku-serve`'s `cache_root::cache_root` uses and for the
/// same reason: every branch is then testable without `std::env::set_var`,
/// which is `unsafe` in edition 2024 and racy under nextest's threads
/// besides. `resolve_db_path`, below, is the caller that reads the real
/// environment and reports what this decided.
///
/// # Errors
///
/// Returns an error if `flag` is absent and neither `xdg` nor `home`
/// answers. There is no relative fallback the way `cache_root` has one: a
/// database that silently follows the working directory is how two runs
/// from two directories come to believe they hold different libraries,
/// which is a worse failure for a store of notes than a slow cache falling
/// back to `.taiyaku-cache`.
fn db_path(
    flag: Option<&str>,
    xdg: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, String> {
    if let Some(dir) = flag.filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    if let Some(xdg) = from_variable(xdg) {
        return Ok(xdg.join("taiyaku").join("library.db"));
    }
    if let Some(home) = from_variable(home) {
        return Ok(home.join(".local/share/taiyaku/library.db"));
    }
    Err(
        "neither --db, XDG_DATA_HOME nor HOME says where the library lives; pass --db PATH"
            .to_string(),
    )
}

/// An environment variable that may be used, or `None` for one that is unset
/// or relative — the same rule `taiyaku-serve`'s `cache_root::from_variable`
/// applies to `XDG_CACHE_HOME`/`HOME`, and for the same reason: the XDG base
/// directory specification says a relative value in one of these variables
/// should be treated as invalid and ignored, and a `--db` resolved from one
/// would silently follow the working directory.
fn from_variable(value: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let path = Path::new(value?);
    path.is_absolute().then(|| path.to_path_buf())
}

/// `db_path`, reading the real environment and reporting what it decided —
/// **every time**, unlike `cache_root`'s warn-only-on-fallback: a cache
/// regenerates and losing track of it costs milliseconds; this database
/// holds a reader's notes and positions, and the whole point of moving it
/// under `XDG_DATA_HOME` — deliberately *data*, not `XDG_CACHE_HOME` — is
/// that a person can find it again, which is worth saying on every run
/// rather than only the run that would have surprised them.
///
/// Also creates the resolved path's parent directory, the same reason
/// `cache_root::create` does: `rusqlite::Connection::open` cannot create a
/// missing directory, and this is the first thing in this codebase ever to
/// open a *real* on-disk library, so `~/.local/share/taiyaku/` will not
/// already exist on a fresh install.
///
/// # Errors
///
/// Returns an error if `flag` is absent and neither `$XDG_DATA_HOME` nor
/// `$HOME` is set (see `db_path`), or if the resolved path's parent
/// directory cannot be created.
pub(crate) fn resolve_db_path(flag: Option<&str>) -> Result<PathBuf, String> {
    let xdg = std::env::var_os("XDG_DATA_HOME");
    let home = std::env::var_os("HOME");
    let path = db_path(flag, xdg.as_deref(), home.as_deref())?;
    eprintln!("taiyaku: library database at {}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests;

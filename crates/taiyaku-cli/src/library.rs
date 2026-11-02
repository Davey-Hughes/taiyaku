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
//! `taiyaku-serve::works::row_id_for_write` mints a placeholder
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

use crate::{flag_present, option_value, reject_positional_arguments, reject_unknown_flags};

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
        Some("scan") => scan(&args[1..]),
        Some("libraries") => libraries(&args[1..]),
        Some("rules") => rules(&args[1..]),
        Some("proposals") => proposals(&args[1..]),
        Some("user") => crate::user::command(&args[1..]),
        Some("key") => crate::user::key_command(&args[1..]),
        Some("root") => root_command(&args[1..]),
        Some("backup") => backup(&args[1..]),
        Some(other) => Err(format!(
            "library has no subcommand `{other}`; it has import, export, scan, libraries, \
             rules, proposals, user, key, root and backup"
        )),
        None => Err(
            "library needs a subcommand: import, export, scan, libraries, rules, proposals, \
             user, key, root or backup"
                .to_string(),
        ),
    }
}

/// `taiyaku library backup [--out DIR] [--keep N] [--db PATH]` — store spec
/// §13.4 and spec C4 §5.
///
/// **`--out` writes somewhere the server will not look**, which the usage line
/// says out loud: `GET /api/library/backups` lists
/// `taiyaku_library::backup::directory_for(db)` and can only compute that one
/// directory. The flag is an escape hatch for an operator writing to another
/// volume, not a second configured location.
///
/// **`--keep` is absent by default.** A command that deletes files the first
/// time it runs is a surprise on the run where the operator is least sure what
/// it does; retention is opt-in, and the cron entry that wants it says so.
///
/// **No schedule**, per build design §5 and spec C4's: a cron entry runs this,
/// and the server has no timer for it.
///
/// # Errors
///
/// Returns an error if a flag is unknown or malformed, if the database cannot
/// be opened, if `VACUUM INTO` fails — which includes a backup of that name
/// already existing — or if a pruned file cannot be removed.
fn backup(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library backup", args, &["--db", "--out", "--keep"])?;
    reject_positional_arguments(
        "library backup",
        args,
        &["--db", "--out", "--keep"],
        "it takes flags only",
    )?;
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let into = match option_value(args, "--out", "a directory")? {
        Some(dir) => PathBuf::from(dir),
        None => taiyaku_library::backup::directory_for(&db),
    };
    let keep = match option_value(args, "--keep", "a count")? {
        Some(n) => Some(
            n.parse::<usize>()
                .map_err(|_| format!("`--keep {n}` is not a count"))?,
        ),
        None => None,
    };

    let library = Library::open(&db)?;
    let taken = {
        let conn = library.conn();
        // **A collision is the caller's timing, not a fault**, and the CLI
        // says so in the terms a person at a terminal is in: the name carries
        // a whole-second stamp, so two runs inside one second have nowhere to
        // put the second. The route answers 409 for the same reason.
        taiyaku_library::backup::take(&conn, &into).map_err(|e| match e {
            taiyaku_library::backup::TakeError::Exists(why) => format!("{why}. Try again"),
            taiyaku_library::backup::TakeError::Failed(why) => why,
        })?
    };
    println!(
        "wrote {} ({} bytes)",
        into.join(&taken.name).display(),
        taken.bytes
    );

    if let Some(keep) = keep {
        for name in taiyaku_library::backup::prune(&into, keep)? {
            println!("pruned {}", into.join(&name).display());
        }
    }
    Ok(())
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
        &["--all", "--db", "--dir", "--in-place", "--user"],
    )?;
    let dir = option_value(args, "--dir", "a directory")?.map(PathBuf::from);
    let in_place = flag_present(args, "--in-place");
    let all = flag_present(args, "--all");
    let positional: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    let user = option_value(args, "--user", "an account name")?;
    let usage = "usage: taiyaku library export <work>|--all [--dir DIR|--in-place] \
                 [--user NAME] [--db PATH]";
    if all && !positional.is_empty() {
        return Err(format!("{usage}: --all takes no work name"));
    }
    if !all && positional.len() != 1 {
        return Err(format!("{usage}: name exactly one work, or pass --all"));
    }
    // C2: with neither flag, `export::work` defaults to writing back over
    // `sidecar_path` — the file a work was imported from. That is exactly right
    // when the database is the more current copy, but the file can be ahead of
    // the database, and an unflagged export would silently revert it. Refusing
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
    // **`--in-place` and `--user` together is refused**, before any database is
    // opened. `--in-place` writes over the files a reader was handed, and
    // there is exactly one reader those files belong to. `--dir` writes
    // anybody's state anywhere, which is what a person reaching for `--user`
    // actually wants.
    if in_place && user.is_some() {
        return Err(format!(
            "{usage}: --in-place with --user would write one reader's notes and position \
             over another's own files, and delete the documents they have no state for. \
             Use --dir DIR to export a named reader's state somewhere of its own"
        ));
    }
    // `taiyaku_read::atomic::write` stages its temporary beside the target,
    // so the target directory has to exist before the first document is
    // written; a bare `--dir` naming a directory a caller has not created
    // yet is the ordinary case, not a mistake, so this creates it rather
    // than refusing. `export::work` creates the per-work subdirectory
    // underneath it — `DIR/<slug>/` — which is what stops every work in a
    // library, all of whose sidecars are named `taiyaku.json`, from writing
    // to the same five paths and overwriting each other.
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

    let user_id = resolve_export_user(&conn, user)?;
    export_targets(&conn, &targets, user_id, dir.as_deref()).map(|_| ())
}

/// Which reader's `notes` and `progress` an export writes into the sidecar.
///
/// **A sidecar carries one reader's state** — that is what the format is, and
/// spec A2 §5.1 declined to version it to hold several. So an export has to name
/// a reader, and the ones it did not write are reported by
/// `Exported::other_users` rather than silently dropped.
///
/// With no `--user`, the lowest account id: on every library that has ever
/// existed that is the row migration 1 seeded and `user promote` renamed, which
/// is the person running the command on the host. **Not a constant**: this is a
/// default for a flag.
fn resolve_export_user(conn: &rusqlite::Connection, named: Option<&str>) -> Result<i64, String> {
    use rusqlite::OptionalExtension as _;

    match named {
        Some(name) => conn
            .query_row("SELECT id FROM user WHERE name = ?1", [name], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| format!("looking up the account `{name}`: {e}"))?
            .ok_or_else(|| format!("no account named `{name}` in this library")),
        None => conn
            // `MIN` over an empty table answers one row holding NULL rather than
            // no rows, so it is typed `Option<i64>` and the NULL is what
            // reaches the "no accounts" error below.
            .query_row("SELECT MIN(id) FROM user", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .map_err(|e| format!("looking up this library's accounts: {e}"))?
            .ok_or_else(|| "this library holds no accounts at all".to_string()),
    }
}

/// How long `scan` waits for its own watch probe's event to arrive before
/// deciding a root cannot be watched. Two seconds: long enough for a real
/// filesystem's `inotify` (or its absence) to show up, short enough that
/// scanning several roots that cannot be watched does not cost minutes
/// finding that out one at a time — `watch::report_line`'s own doc comment
/// uses this exact figure in its worked example.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// `taiyaku library scan`: walk every known root, or one library's under
/// `--library NAME`, or one root under `--root PATH`; record what §8's report
/// says about each, derive series, works, editions and members from the
/// facts, and raise whatever proposals the new structure supports.
/// `--no-derive` stops after the walk, which is what makes "what would the
/// facts be" answerable without changing structure.
///
/// **`--add PATH` files the directory under `--library NAME`**, creating that
/// library where no library has the name, **and under a library of its own
/// where `--library` is absent**, and prints which library that is, by the
/// name the store holds, and whether the add created it. The directories to walk
/// are chosen after the add, so `--add PATH --library NAME` walks `NAME`'s
/// directories, the new one among them.
///
/// **A root that cannot be scanned does not take the others down with it,
/// but it does take the exit status down with it.** Neither an outright
/// error from [`taiyaku_library::scan::scan_root`] (a permission refusal,
/// say) nor §8.1's empty-root refusal aborts the loop — each is named on
/// stderr and the run moves on to the next root, the same "write what can be
/// written" trade [`export_targets`] makes. But a scan whose only root
/// refused must not exit 0 over having done nothing — that is the no-roots
/// refusal's own failure arriving by another door, and a script watching the
/// exit code cannot see stderr. So every failure, refused or errored, is
/// counted, and if any root failed this returns `Err` naming how many of how
/// many, once every root has had its turn — [`export_targets`]'s own
/// precedent, applied here.
fn scan(args: &[String]) -> Result<(), String> {
    reject_unknown_flags(
        "library scan",
        args,
        &["--add", "--library", "--root", "--no-derive", "--db"],
    )?;
    reject_positional_arguments(
        "library scan",
        args,
        &["--add", "--library", "--root", "--db"],
        "a path reaches this command as `--add PATH` (a root it does not \
         know yet) or `--root PATH` (one it does)",
    )?;
    let add = option_value(args, "--add", "a path")?;
    let root_filter = option_value(args, "--root", "a path")?;
    let library_name = option_value(args, "--library", "a library name")?;
    if root_filter.is_some() && library_name.is_some() {
        return Err(
            "`--root` and `--library` each choose which directories are walked; pass one of them"
                .to_string(),
        );
    }
    let no_derive = flag_present(args, "--no-derive");
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;

    if let Some(path) = add {
        // The checked add, the one `POST /api/library/roots` makes: a root
        // inside or around one already held has the overlap walked twice, and
        // derive admits the same directory as two members of one work (O47).
        let conn = library.conn();
        // Asked before the add, which makes the library where none answers.
        let (filed, joined) = match library_name {
            Some(name) => (
                taiyaku_library::scan::InLibrary::Named(name),
                taiyaku_library::libraries::id_of(&conn, name)?,
            ),
            None => (taiyaku_library::scan::InLibrary::Own, None),
        };
        let added = taiyaku_library::scan::add_root_checked_in(&conn, Path::new(path), filed)
            .map_err(|e| format!("adding root {path}: {e}"))?;
        println!(
            "{}",
            added_line(
                &stored_name(&conn, added.root.library_id)?,
                joined.is_none()
            )
        );
    }

    let roots = {
        let conn = library.conn();
        taiyaku_library::scan::roots(&conn)?
    };
    let library_filter = match library_name {
        Some(name) => Some(
            taiyaku_library::libraries::id_of(&library.conn(), name)?.ok_or_else(|| {
                format!("no library named `{name}`; `taiyaku library libraries` lists them")
            })?,
        ),
        None => None,
    };
    let selected = select_roots(roots, root_filter, library_filter)?;
    let total = selected.len();
    let mut failed: Vec<String> = Vec::new();

    for root in &selected {
        let report = {
            let conn = library.conn();
            taiyaku_library::scan::scan_root(&conn, root)
        };
        let report = match report {
            Ok(report) => report,
            Err(e) => {
                eprintln!("taiyaku: {}: {e}", root.path.display());
                failed.push(root.path.display().to_string());
                continue;
            }
        };
        if let Some(refusal) = &report.refused {
            eprintln!("taiyaku: {refusal}");
            failed.push(root.path.display().to_string());
            continue;
        }
        let probe = taiyaku_library::watch::probe(&root.path, PROBE_TIMEOUT);
        let interval = taiyaku_library::watch::interval_for(&probe);
        println!(
            "{}",
            taiyaku_library::watch::report_line(&root.path, &probe, report.dirs, interval)
        );
        println!("{}", scan_summary(&report));
    }

    if !no_derive {
        let conn = library.conn();
        // One raiser, like one writer: `derive` already calls
        // `proposals::raise` once, at the end of its own pass, and returns
        // the count it opened.
        let derived =
            taiyaku_library::derive::derive(&conn).map_err(|e| format!("deriving: {e}"))?;
        println!("{}", derive_summary(&derived, derived.raised));
    }

    scan_result(&failed, total)
}

/// The exit status half of §8.1's refusal, matching [`export_targets`]'s own
/// precedent for "some of these failed": every root that could be scanned,
/// was, but a caller relying on the exit code — a cron job, a script — must
/// not read a root that refused or errored as a quiet success. Named on
/// stderr already, as each failure happened; this is the one line that says
/// so where an exit code can see it.
fn scan_result(failed: &[String], total: usize) -> Result<(), String> {
    if failed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} of {total} root(s) could not be scanned: {}. The reason for each is on stderr \
         above; what could be scanned, was",
        failed.len(),
        failed.join(", ")
    ))
}

/// Which roots `scan` processes this run: every known root, or the roots of
/// the one library whose id is `library`, or — under `--root PATH` — the
/// single one whose canonical path matches `root`. `scan` refuses `--root`
/// with `--library`, so it passes at most one of the two.
///
/// # Errors
///
/// Returns an error naming `--add` if no `--root` filter was given and there
/// is no root to walk, in the store or in `library` — a scan with nothing to
/// scan is a refusal, not a silent success over an empty library — or naming
/// `root` if `--root PATH` cannot be canonicalised (it must exist) or matches
/// no root this library knows.
fn select_roots(
    roots: Vec<taiyaku_library::scan::Root>,
    root: Option<&str>,
    library: Option<i64>,
) -> Result<Vec<taiyaku_library::scan::Root>, String> {
    let roots = match library {
        Some(id) => roots.into_iter().filter(|r| r.library_id == id).collect(),
        None => roots,
    };
    if let Some(path) = root {
        let canonical = Path::new(path)
            .canonicalize()
            .map_err(|e| format!("--root {path}: {e}"))?;
        let matched: Vec<_> = roots.into_iter().filter(|r| r.path == canonical).collect();
        if matched.is_empty() {
            return Err(format!(
                "no known root at {}; pass --add {} to add it first",
                canonical.display(),
                canonical.display()
            ));
        }
        return Ok(matched);
    }
    if roots.is_empty() {
        return Err("there are no directories to scan; pass --add PATH to add one".to_string());
    }
    Ok(roots)
}

/// `taiyaku library libraries [--db PATH]`: every library, its directories, and
/// how many works each holds, then how many works are in none.
///
/// # Errors
///
/// Returns an error for an unknown flag or a bare word, if the database
/// cannot be opened, or if a read fails.
fn libraries(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library libraries", args, &["--db"])?;
    reject_positional_arguments("library libraries", args, &["--db"], "it takes flags only")?;
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let conn = library.conn();
    let listed = taiyaku_library::libraries::list(&conn)?;
    if let Some(line) = no_libraries_line(listed.len()) {
        println!("{line}");
    }
    for one in &listed {
        println!("{}", library_line(one));
        for dir in &one.dirs {
            println!("  {}", dir.display());
        }
    }
    if let Some(line) = unfiled_line(taiyaku_library::libraries::unfiled_works(&conn)?) {
        println!("{line}");
    }
    Ok(())
}

/// What the listing says where there is no library to list, and the command
/// that makes one. Extracted for the reason [`unfiled_line`] was.
fn no_libraries_line(libraries: usize) -> Option<String> {
    (libraries == 0).then(|| {
        "no libraries yet; `taiyaku library scan --add PATH` adds a directory as one".to_string()
    })
}

/// One library's heading line.
fn library_line(listed: &taiyaku_library::libraries::Listed) -> String {
    let dirs = listed.dirs.len();
    let noun = if dirs == 1 {
        "directory"
    } else {
        "directories"
    };
    format!("{}: {dirs} {noun}, {} work(s)", listed.name, listed.works)
}

/// The works in no library, said only when there are some. Extracted for the
/// reason `user::unrooted_note` was.
fn unfiled_line(works: usize) -> Option<String> {
    (works > 0).then(|| format!("{works} work(s) in no library"))
}

/// What `scan --add` says once the directory is added: the library it is now
/// in, by the name the store holds for it, and whether the add `created`
/// that library or the directory joined one already there.
fn added_line(library: &str, created: bool) -> String {
    if created {
        format!("created the library `{library}` for this directory")
    } else {
        format!("filed under the library `{library}`")
    }
}

/// The name the store holds for the library with this id.
///
/// # Errors
///
/// Returns an error if the read fails, which includes no library having the
/// id.
pub(crate) fn stored_name(conn: &rusqlite::Connection, id: i64) -> Result<String, String> {
    conn.query_row("SELECT name FROM library WHERE id = ?1", [id], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading the name of library {id}: {e}"))
}

const DOCUMENTS_USAGE: &str = "usage: taiyaku library root documents <path> on|off [--db PATH]";

/// Dispatch `library root`.
///
/// **One subcommand today, dispatched the same shape [`crate::user::command`]
/// uses for `library user`** — a `root` group rather than a bare
/// `library documents` command, because this library's other per-root settings
/// (`edition_key`, `default_kind`, `default_language`, `scan_interval_secs`)
/// would need a home too, and this is the one place a second root-scoped
/// command joins without reshaping the dispatch again.
///
/// # Errors
///
/// Returns an error naming the subcommand if none matches, or whatever the
/// subcommand itself returns.
fn root_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("documents") => root_documents(&args[1..]),
        Some(other) => Err(format!(
            "library root has no subcommand `{other}`; it has documents"
        )),
        None => Err("library root needs a subcommand: documents".to_string()),
    }
}

/// `taiyaku library root documents <PATH> on|off [--db PATH]`: flip
/// `root.keeps_documents` — migration 012's column, off by default since
/// `docs/superpowers/specs/2026-09-02-hand-documents-off-by-default-design.md`
/// — for the one root `PATH` names, and nothing else.
///
/// **Changes one column and stops.** It does not scan, does not derive, and
/// does not write, move or delete a single document. Turning documents on
/// gets them written by the next `taiyaku library scan`'s `hand::reconcile`
/// pass rather than by a second write path minted here. See
/// [`documents_state_line`] for why the printed line says so in full: the
/// reasonable reading of "now keeps documents" is that a file just
/// appeared, and it has not.
///
/// # Errors
///
/// Returns an error for an unknown flag, for anything other than exactly one
/// path and one of `on`/`off`, if `PATH` cannot be canonicalised or names no
/// root of this library (see [`root_named`]), or if the database cannot be
/// opened.
fn root_documents(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library root documents", args, &["--db"])?;
    let positional: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    let [path, state] = positional.as_slice() else {
        return Err(DOCUMENTS_USAGE.to_string());
    };
    let on = match state.as_str() {
        "on" => true,
        "off" => false,
        other => return Err(format!("`{other}` is not `on` or `off`")),
    };
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let conn = library.conn();
    let root = root_named(&conn, path)?;
    conn.execute(
        "UPDATE root SET keeps_documents = ?1 WHERE id = ?2",
        rusqlite::params![on, root.id],
    )
    .map_err(|e| format!("setting keeps_documents for {}: {e}", root.path.display()))?;
    println!("{}", documents_state_line(&root.path, on));
    Ok(())
}

/// The one root of this library whose canonicalised path is `given` —
/// resolved the same way [`select_roots`] resolves `scan --root`, so
/// `./library` and its absolute spelling name the same row here too.
///
/// **Does not fall back to a stored, uncanonicalisable path**: there is
/// nothing for `hand::reconcile` to write into on a root that cannot be
/// found, so this command's requirement is exactly `--root`'s.
///
/// # Errors
///
/// Returns an error naming `given` if it cannot be canonicalised (the path
/// must exist), or if no known root's stored path matches it.
fn root_named(
    conn: &rusqlite::Connection,
    given: &str,
) -> Result<taiyaku_library::scan::Root, String> {
    let canonical = Path::new(given)
        .canonicalize()
        .map_err(|e| format!("{given}: {e}"))?;
    taiyaku_library::scan::roots(conn)?
        .into_iter()
        .find(|root| root.path == canonical)
        .ok_or_else(|| {
            format!(
                "no known root at {}; `taiyaku library scan --add {}` would add it first",
                canonical.display(),
                canonical.display()
            )
        })
}

/// What [`root_documents`] prints once the column is set — pulled into a
/// pure function for the reason `crate::user::sessions_ended_note` and its
/// siblings give: a branch inside a `println!` is reachable only by running
/// the command.
///
/// **States what the flag means, not only what changed.** `on`/`off` alone
/// tells a reader nothing unless they already know what `keeps_documents`
/// governs; the second sentence is the fact a person actually wants — whether
/// a hand correction now has a copy that outlives the database, or lives in
/// the database alone.
///
/// **States plainly that nothing was just written.** The reasonable reading
/// of "this root now keeps documents" is that a file appeared, and it has
/// not — `root_documents` changes this one column and nothing else; the
/// document itself is `hand::reconcile`'s to write, the next time
/// `taiyaku library scan` runs it.
fn documents_state_line(path: &Path, on: bool) -> String {
    if on {
        format!(
            "{} now keeps documents: a hand edit is written beside the archives, at \
             <root>/.taiyaku/, and survives losing the database. Nothing is written yet — \
             the next `taiyaku library scan` writes it, through `hand::reconcile`, the same \
             as any other scan",
            path.display()
        )
    } else {
        format!(
            "{} no longer keeps documents: corrections live in the database only. Any \
             document already on disk is left exactly where it is — this does not delete it",
            path.display()
        )
    }
}

/// The counts one [`taiyaku_library::scan::scan_root`] call produced, beside
/// [`taiyaku_library::watch::report_line`]'s own line — the two together are
/// §8's report for one root.
///
/// `unreadable_dirs`, `excluded_dirs`, `excluded_files`,
/// `forgotten_by_exclusion` and `bad_exclude_patterns` print only when
/// non-zero, the same rule [`derive_summary`] applies to its own
/// silent-outcome counters: a `Vec` nobody prints is a directory a person
/// never learns was skipped.
fn scan_summary(report: &taiyaku_library::scan::ScanReport) -> String {
    use std::fmt::Write as _;

    let mut line = format!(
        "  {} added, {} updated, {} gone",
        report.added, report.updated, report.gone
    );
    if !report.unreadable_dirs.is_empty() {
        let _ = write!(
            line,
            "; {} unreadable directories: {}",
            report.unreadable_dirs.len(),
            report.unreadable_dirs.join(", ")
        );
    }
    if !report.excluded_dirs.is_empty() {
        let _ = write!(
            line,
            "; {} directories excluded: {}",
            report.excluded_dirs.len(),
            report.excluded_dirs.join(", ")
        );
    }
    if report.excluded_files > 0 {
        let _ = write!(line, "; {} files excluded", report.excluded_files);
    }
    if report.forgotten_by_exclusion > 0 {
        let _ = write!(
            line,
            "; {} entries forgotten because a pattern now covers them",
            report.forgotten_by_exclusion
        );
    }
    if !report.bad_exclude_patterns.is_empty() {
        let _ = write!(
            line,
            "; {} unusable exclude pattern(s): {}",
            report.bad_exclude_patterns.len(),
            report.bad_exclude_patterns.join(", ")
        );
    }
    line
}

/// Why [`derive_summary`]'s unreadable entries were skipped, one clause per
/// cause that happened — O90.
///
/// **A refused format prints as its count and its token** (`12 rar`), the
/// schema's own vocabulary, for the reason `taiyaku_read::Format::token`
/// gives: a friendlier spelling would be a second name for a value the
/// database already names. **The two causes that are single faulty files
/// print their paths**, joined on `; ` like every other list on the line,
/// because a count alone leaves a person querying the database for the file.
/// A container holding no page images is a layout rather than a fault, so it
/// is only counted.
///
/// Every clause agrees with one and with many alike, for the reason
/// `series_holding_no_work`'s wording gives below.
fn unreadable_causes(unreadable: &taiyaku_library::derive::Unreadable) -> String {
    let mut causes: Vec<String> = unreadable
        .by_format
        .iter()
        .map(|(format, n)| format!("{n} {format}"))
        .collect();
    if !unreadable.uncounted.is_empty() {
        causes.push(format!(
            "{} whose pages could not be counted: {}",
            unreadable.uncounted.len(),
            unreadable.uncounted.join("; ")
        ));
    }
    if !unreadable.vanished.is_empty() {
        causes.push(format!(
            "{} gone before they could be hashed: {}",
            unreadable.vanished.len(),
            unreadable.vanished.join("; ")
        ));
    }
    if unreadable.no_pages > 0 {
        causes.push(format!("{} holding no page images", unreadable.no_pages));
    }
    causes.join(", ")
}

/// What one `derive` + `raise` pass over the whole library produced.
///
/// `series`/`works`/`editions`/`members` print unconditionally — a library
/// that genuinely derived nothing is exactly as informative a zero as any
/// other. `unattached_chapter_dirs` and `unreadable` print only when
/// non-zero (`unreadable` with its causes — see [`unreadable_causes`]), and
/// so do `suppressed`, `rekeyed`, `kept_editions`, `merged`,
/// `unrecognised_kinds`, `emptied_works` (with the counts of what its fold
/// wrote, what stopped existing, what it dropped as a copy the target already
/// held identically, and what it discarded), `unfolded_works`,
/// `unreadable_hand`, `series_holding_no_work`, `repaired_documents`,
/// `unwritable_documents`, `orphan_documents`, `declined_documents` and
/// `stale_documents`:
/// [`taiyaku_library::derive::DeriveReport`]'s own doc comment is explicit
/// that each exists because a silent drop, a silent re-key, a silently
/// orphaned edition, a silent merge, a silently refused edition kind or a
/// hand document derivation could not read is indistinguishable from an empty
/// library, from growth, from a clean derive, from a work that genuinely
/// holds one volume, from a rule that never named a kind, or from a
/// correction that was never written — a counter that never reaches this
/// line is the failure they were added to prevent.
///
/// **`emptied_works` is the sharpest of them**, being the only derive outcome
/// that deletes a `work` row: what moved with it, how many reading positions
/// stopped existing, what a labeller already held that it left alone, and what
/// its `DELETE` took and did not move all travel on the same line, so
/// "1 work folded away" can never stand on its own.
///
/// **The two position clauses sit together**, so the one number that is
/// about a reading position destroyed is not read in the middle of two
/// clauses about labelling. Grouping is the only thing a single line of prose
/// has instead of columns.
///
/// **`unfolded_works` prints its reasons beside its count**, the way
/// [`scan_summary`] prints `unreadable_dirs`: the refusal messages name the
/// work and the edition keys, and a bare "1 superseded work left standing"
/// over a library of thousands tells a person something happened and gives
/// them nothing to look at.
///
/// **`series_holding_no_work` says "holding no work" and not "left empty".**
/// The field is a whole-library census taken after the folds — `derive` mints
/// a series row for every named series, a series holding only unattached
/// chapter directories among them — so on a first scan of an ordinary chapter
/// tree "left empty" would sit on the line reporting what a fold did and read
/// as a claim about this run, with nothing having been emptied by anything.
/// The present participle also agrees with one series and with six alike — no
/// singular/plural branch, for the reason `fold::absorb`'s `edition(s)` gives.
/// The count is a census, deliberately so — see
/// `DeriveReport::series_holding_no_work`, which argues for it.
fn derive_summary(report: &taiyaku_library::derive::DeriveReport, raised: usize) -> String {
    use std::fmt::Write as _;

    let mut line = format!(
        "derived {} series, {} works, {} editions, {} members",
        report.series, report.works, report.editions, report.members
    );
    if report.unattached_chapter_dirs > 0 {
        let _ = write!(
            line,
            ", {} unattached chapter directories",
            report.unattached_chapter_dirs
        );
    }
    if report.unreadable.total() > 0 {
        let _ = write!(
            line,
            ", {} unreadable ({})",
            report.unreadable.total(),
            unreadable_causes(&report.unreadable)
        );
    }
    if report.suppressed > 0 {
        let _ = write!(line, ", {} suppressed", report.suppressed);
    }
    if report.rekeyed > 0 {
        let _ = write!(line, ", {} rekeyed", report.rekeyed);
    }
    if report.kept_editions > 0 {
        let _ = write!(line, ", {} editions kept", report.kept_editions);
    }
    if report.merged > 0 {
        let _ = write!(line, ", {} merged", report.merged);
    }
    if report.unrecognised_kinds > 0 {
        let _ = write!(
            line,
            ", {} unrecognised edition kinds",
            report.unrecognised_kinds
        );
    }
    if report.emptied_works > 0 {
        let _ = write!(
            line,
            ", {} work(s) folded away ({} position(s) written, {} position(s) lost, \
             {} note(s) and {} truth page(s) moved, {} identical note(s) dropped, \
             {} truth page(s) already held, {} row(s) discarded, \
             {} alignment(s) not carried)",
            report.emptied_works,
            report.moved_positions,
            report.lost_positions,
            report.moved_notes,
            report.moved_truth_pages,
            report.dropped_notes,
            report.moved_truth_collisions,
            report.discarded_rows,
            report.uncarried_alignments,
        );
    }
    if report.unfolded_works > 0 {
        let _ = write!(
            line,
            ", {} superseded work(s) left standing: {}",
            report.unfolded_works,
            report.unfoldable.join("; ")
        );
    }
    if !report.unreadable_hand.is_empty() {
        let _ = write!(
            line,
            ", {} hand document(s) unreadable: {}",
            report.unreadable_hand.len(),
            report.unreadable_hand.join("; ")
        );
    }
    if report.series_holding_no_work > 0 {
        let _ = write!(
            line,
            ", {} series holding no work",
            report.series_holding_no_work
        );
    }
    if report.repaired_documents > 0 {
        let _ = write!(
            line,
            ", {} document(s) written that were not on disk",
            report.repaired_documents
        );
    }
    if !report.unwritable_documents.is_empty() {
        let _ = write!(
            line,
            ", {} document(s) still not durable: {}",
            report.unwritable_documents.len(),
            report.unwritable_documents.join("; ")
        );
    }
    if !report.orphan_documents.is_empty() {
        // Path **and** title: the filename is a `content_key` naming content
        // that no longer exists, so the path alone gives a person nothing to
        // act on.
        //
        // **"No work owes it" here means what it says: nothing addresses
        // it.** A document a live work still addresses, whose root has
        // simply declined to keep it, is reported below under
        // `declined_documents` instead. Folding the two together would tell a
        // person who turned documents off, on every scan, that *every* hand
        // document in the library was stranded and owed to nothing, sending
        // them hunting for (or deleting) works that are perfectly fine; see
        // `taiyaku_library::hand::Orphan`'s own doc comment.
        //
        // **A fold's target is appended, not substituted.** The path and
        // title stay exactly as they were for every other orphan, and a
        // person who reads "here is a hash and a title" still gets that;
        // when a fold is why the document is stranded, `absorbed_into` gives
        // them a second, better thing to act on — the slug to go and look
        // at, and the title to recognise it by. See
        // `taiyaku_library::hand::Orphan::absorbed_into` for why this names
        // the target rather than moving the document there.
        let named: Vec<String> = report
            .orphan_documents
            .iter()
            .map(|orphan| {
                let mut text = match &orphan.title {
                    Some(title) => format!("{} (`{title}`)", orphan.path.display()),
                    None => orphan.path.display().to_string(),
                };
                if let Some(absorbed) = &orphan.absorbed_into {
                    let _ = write!(
                        text,
                        ", now held as an edition of `{}` (`{}`)",
                        absorbed.slug, absorbed.title
                    );
                }
                text
            })
            .collect();
        let _ = write!(
            line,
            ", {} document(s) no work owes: {}",
            named.len(),
            named.join("; ")
        );
    }
    if !report.declined_documents.is_empty() {
        // **Worded so it reads as a standing fact about this root, not a
        // loss.** "No work owes" — the orphan clause above — says a document
        // is stranded and invites a person to act on it; a declined document
        // is neither. "This root no longer keeps" names what is so:
        // `keeps_documents` is off, by someone's choice or by migration 012's
        // default, and the file at a perfectly good address is simply outside
        // what this root maintains.
        let named: Vec<String> = report
            .declined_documents
            .iter()
            .map(|declined| match &declined.title {
                Some(title) => format!("{} (`{title}`)", declined.path.display()),
                None => declined.path.display().to_string(),
            })
            .collect();
        let _ = write!(
            line,
            ", {} document(s) this root no longer keeps: {}",
            named.len(),
            named.join("; ")
        );
    }
    if !report.stale_documents.is_empty() {
        let _ = write!(
            line,
            ", {} document(s) disagree with the database: {}",
            report.stale_documents.len(),
            report.stale_documents.join("; ")
        );
    }
    let _ = write!(line, "; {raised} proposal(s) raised");
    line
}

/// `taiyaku library rules`: print this library's rule set as JSON
/// (`--export`, or nothing — the default is "show me what is there"), or
/// replace it wholesale from a file (`--import FILE`). §4: rules travel as
/// JSON rather than as a config file, because a config file would be a second
/// source of truth a UI writing rules would have to keep in step.
fn rules(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library rules", args, &["--export", "--import", "--db"])?;
    reject_positional_arguments(
        "library rules",
        args,
        &["--import", "--db"],
        "the rule set goes out on stdout with no argument at all, and comes \
         back in as `--import FILE`",
    )?;
    let import_path = option_value(args, "--import", "a file")?;
    let export_flag = flag_present(args, "--export");
    if export_flag && import_path.is_some() {
        return Err("library rules takes --export or --import, not both".to_string());
    }

    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let conn = library.conn();

    if let Some(path) = import_path {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        // The return value is every accepted proposal `import_json` itself
        // just reopened, because the rule it had accepted did not survive
        // the replace. Named here rather than only inside the library, the
        // same discipline the paragraph below applies to "nothing has been
        // re-derived yet".
        let reopened = taiyaku_library::rules::import_json(&conn, &text)
            .map_err(|e| format!("importing rules from {path}: {e}"))?;
        // **An import replaces the rule set and derives nothing.** The whole
        // claim rules make is that editing one re-derives the library without
        // touching the filesystem — so a person who has just replaced every
        // rule and seen only "imported" reasonably believes the library now
        // reflects them, and it does not until something calls `derive`. The
        // command that does is named here rather than left to be found, and
        // `--no-derive` is deliberately not offered as the answer: it is the
        // flag that stops the very thing this line is asking for.
        println!(
            "imported rules from {path}\n  \
             the rules are stored; nothing has been re-derived from them yet — \
             run `taiyaku library scan` to apply them"
        );
        if !reopened.is_empty() {
            println!(
                "  {} accepted proposal(s) lost the rule they were accepted by and are open \
                 again: {reopened:?}",
                reopened.len()
            );
        }
        return Ok(());
    }

    let json =
        taiyaku_library::rules::export_json(&conn).map_err(|e| format!("exporting rules: {e}"))?;
    println!("{json}");
    Ok(())
}

/// `taiyaku library proposals [--min-score N] [--accept ID] [--dismiss ID]
/// [--unaccept ID] [--reopen ID] [--db PATH]`: the surface over
/// `taiyaku_library::proposals` — [`taiyaku_library::proposals::open`],
/// [`taiyaku_library::proposals::accept`], [`taiyaku_library::proposals::dismiss`],
/// [`taiyaku_library::proposals::unaccept`] and
/// [`taiyaku_library::proposals::reopen`].
///
/// **Listing is the default**, with nothing after it: `library scan` already
/// raises proposals as a side effect of deriving, so the ordinary use of this
/// command is "what is in the inbox right now", not an action. `--min-score`
/// narrows it the same way [`taiyaku_library::proposals::open`]'s own
/// argument does — §6.1's threshold, not a suppression list.
///
/// **At most one of `--accept`, `--dismiss`, `--unaccept` and `--reopen`.** Each names a
/// proposal and performs the one action a person asked for; taking more than
/// one at once would leave which happened first, and to which id, ambiguous
/// the moment two flags named different proposals.
///
/// **Refusals are never reworded here.** `taiyaku_library::proposals::accept`
/// already says why it refuses in full — naming `taiyaku align` for an
/// `attach-chapters` proposal, or naming exactly what a swallowed directory
/// would have taken — and this command propagates that message rather than
/// summarising it, the same discipline every other subcommand in this module
/// already keeps for the library crate's own errors.
///
/// # Errors
///
/// Returns an error for an unknown flag, for more than one of
/// `--accept`/`--dismiss`/`--unaccept`/`--reopen`, for an id or a `--min-score` that will
/// not parse as a number, if the database cannot be opened, or whatever the
/// underlying `taiyaku_library::proposals` call itself returns.
fn proposals(args: &[String]) -> Result<(), String> {
    reject_unknown_flags(
        "library proposals",
        args,
        &[
            "--min-score",
            "--accept",
            "--dismiss",
            "--unaccept",
            "--reopen",
            "--db",
        ],
    )?;
    reject_positional_arguments(
        "library proposals",
        args,
        &[
            "--min-score",
            "--accept",
            "--dismiss",
            "--unaccept",
            "--reopen",
            "--db",
        ],
        "a proposal is named by id, as `--accept ID`, `--dismiss ID`, \
         `--unaccept ID` or `--reopen ID`",
    )?;

    let accept_id = option_value(args, "--accept", "a proposal id")?;
    let dismiss_id = option_value(args, "--dismiss", "a proposal id")?;
    let unaccept_id = option_value(args, "--unaccept", "a proposal id")?;
    let reopen_id = option_value(args, "--reopen", "a proposal id")?;
    let min_score = option_value(args, "--min-score", "a number")?;

    let actions = [accept_id, dismiss_id, unaccept_id, reopen_id]
        .iter()
        .filter(|a| a.is_some())
        .count();
    if actions > 1 {
        return Err(
            "library proposals takes at most one of --accept, --dismiss, --unaccept or \
             --reopen at a time"
                .to_string(),
        );
    }

    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let conn = library.conn();

    if let Some(id) = accept_id {
        let id = parse_proposal_id(id)?;
        let rule_id = taiyaku_library::proposals::accept(&conn, id)?;
        println!("accepted proposal {id}: wrote rule {rule_id}");
        return Ok(());
    }
    // **A refusal is this command's error, unreworded.** `dismiss` and
    // `unaccept` answer one as `Offer::Refused` (O70), for `taiyaku-serve`,
    // which gives a refusal and a store failure different statuses; this
    // command prints either and exits 1, so it folds the two together here, as
    // `proposals::accept` does for `--accept`.
    if let Some(id) = dismiss_id {
        let id = parse_proposal_id(id)?;
        if let taiyaku_library::proposals::Offer::Refused(why) =
            taiyaku_library::proposals::dismiss(&conn, id)?
        {
            return Err(why);
        }
        println!("dismissed proposal {id}");
        return Ok(());
    }
    if let Some(id) = unaccept_id {
        let id = parse_proposal_id(id)?;
        if let taiyaku_library::proposals::Offer::Refused(why) =
            taiyaku_library::proposals::unaccept(&conn, id)?
        {
            return Err(why);
        }
        println!("un-accepted proposal {id}, and removed the rule it had written");
        return Ok(());
    }
    // The undo of `--dismiss` (O72), folded back into this command's error the
    // same way when the library refuses it.
    if let Some(id) = reopen_id {
        let id = parse_proposal_id(id)?;
        if let taiyaku_library::proposals::Offer::Refused(why) =
            taiyaku_library::proposals::reopen(&conn, id)?
        {
            return Err(why);
        }
        println!("reopened proposal {id}");
        return Ok(());
    }

    let min_score = match min_score {
        Some(s) => s
            .parse::<f64>()
            .map_err(|_| format!("--min-score needs a number, and `{s}` is not one"))?,
        None => 0.0,
    };
    let open = taiyaku_library::proposals::open(&conn, min_score)
        .map_err(|e| format!("listing proposals: {e}"))?;
    if open.is_empty() {
        println!("no open proposals at or above score {min_score}");
        return Ok(());
    }
    for proposal in &open {
        println!("{}", proposal_line(proposal));
    }
    Ok(())
}

/// One line of `taiyaku library proposals`' default listing: the id and kind
/// a `--accept`/`--dismiss`/`--unaccept`/`--reopen` call takes, the score
/// `--min-score` filters on, and `reason` — the human-readable sentence
/// [`taiyaku_library::proposals::raise`] itself wrote, which already names
/// the series involved, so this does not have to look them up a second time.
///
/// Pulled out as its own pure function for the reason `scan_summary` and
/// `derive_summary` are: a `println!` inline in `proposals` cannot be
/// asserted on directly, and the format is exactly the kind of thing that
/// silently drifts once nothing pins it.
fn proposal_line(proposal: &taiyaku_library::proposals::Proposal) -> String {
    format!(
        "{} [{}] score={:.1}: {}",
        proposal.id, proposal.kind, proposal.score, proposal.reason
    )
}

/// A `--accept`/`--dismiss`/`--unaccept`/`--reopen` argument, parsed as the `proposal.id`
/// it names.
///
/// # Errors
///
/// Returns an error naming `raw` if it will not parse as an integer.
fn parse_proposal_id(raw: &str) -> Result<i64, String> {
    raw.parse::<i64>()
        .map_err(|_| format!("`{raw}` is not a proposal id (a whole number)"))
}

/// Export every named work, and report what happened to each.
///
/// **A work that fails does not abort the ones after it.** Propagating with
/// `?` would leave, under `--all`, the works before a single bad work — a NULL
/// `sidecar_path`, which `clear_stale_path_claim` can produce, most plausibly —
/// written and the works after it neither written nor named. Under
/// `--in-place` that is the worst available outcome: some of a reader's
/// sources overwritten from the database, the rest not, and no list of which.
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
/// `Result<(), String>`: a figure that lives only in a `println!` cannot be
/// asserted, and a test needs a value. Its one production caller drops it —
/// the command's output is the printed line — but a claim a person reads off
/// the terminal after overwriting their own library is worth a test.
fn export_targets(
    conn: &rusqlite::Connection,
    targets: &[(i64, String)],
    user_id: i64,
    dir: Option<&Path>,
) -> Result<usize, String> {
    let mut total = 0usize;
    let mut failed: Vec<&str> = Vec::new();
    for (id, slug) in targets {
        match taiyaku_library::export::work(conn, *id, user_id, dir) {
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
                // **It cannot promise "a fresh one".** A document with no state
                // behind it is not written at all — the rule `export::work`
                // applies to all of them but `taiyaku.notes.json`,
                // `taiyaku.library.json` included — and an empty database side
                // is exactly the state that produces a `kept` in the first
                // place. So moving the file aside can leave the reader with
                // nothing, where "a fresh one" would promise a skeleton to fill
                // in. What it can honestly say is what the export would put
                // there, including when that is nothing.
                for path in &exported.kept {
                    println!(
                        "kept {} (this library has never read it, so it was not \
                         overwritten; move it aside and export again to replace it with \
                         what this library holds — which is no file at all where it \
                         holds nothing)",
                        path.display()
                    );
                }
                // The fourth outcome, and the one nothing else here can say:
                // a sidecar carries one reader's notes and position, so an
                // export on a shared library leaves the other readers' behind.
                // Not an error and not a refusal — one set per file is the
                // format, not a fault — but it is a difference between
                // "exported" and "exported everything", and a person reading
                // the terminal is entitled to know which they got (§5.1).
                if !exported.other_users.is_empty() {
                    println!(
                        "note: {} other reader(s) hold notes or a position for `{slug}` \
                         and are not in these files: {}. A sidecar carries one reader's \
                         state; `--dir DIR --user NAME` exports theirs somewhere of its \
                         own (not --in-place, which would write over these)",
                        exported.other_users.len(),
                        exported.other_users.join(", ")
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
/// call to `taiyaku_library::migrate::work` read back from a work's up to
/// four sibling documents on its first import. A document with no sibling file
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
/// **Returns the lines rather than printing them**, so a test can assert them.
/// That matters most for a malformed `taiyaku.truth.json`, which is left on
/// disk *so that* fixing it and importing again works: this line is how a
/// reader learns there is something to fix. `migration_summary` above has
/// this shape for the same reason.
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
    // The importing account, by the same rule `resolve_export_user`
    // documents: the sidecar's own notes and progress belong to whoever is
    // running the import on the host.
    let user_id = resolve_export_user(&conn, None)?;
    let migration = taiyaku_library::migrate::work(&conn, work_id, user_id, canonical)?;
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
/// holds a reader's notes and positions, and the whole point of putting it
/// under `XDG_DATA_HOME` — deliberately *data*, not `XDG_CACHE_HOME` — is
/// that a person can find it again, which is worth saying on every run
/// rather than only the run that would have surprised them.
///
/// Also creates the resolved path's parent directory, the same reason
/// `cache_root::create` does: `rusqlite::Connection::open` cannot create a
/// missing directory, and `~/.local/share/taiyaku/` does not exist on a
/// fresh install.
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

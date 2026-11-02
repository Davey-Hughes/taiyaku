//! What `main` itself does, which no unit test can reach.
//!
//! Everything below `run` is tested in place. `main` is the four lines around
//! it — take the arguments, call it, print the message, set the exit code —
//! and each of those four is a contract something outside this repository
//! depends on: `scripts/margin-survey.sh` and the collectors beside it read
//! stdout and branch on the status. A `main` that did nothing at all would
//! leave every other test in the crate green.

use std::path::Path;
use std::process::Command;
use taiyaku_read::test_support::TempDir;

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

/// Runs the binary with `args`, which must succeed, and returns its stdout
/// alone. A failing exit panics with what the command printed to both
/// streams.
fn taiyaku_stdout(args: &[&str]) -> String {
    let run = taiyaku().args(args).output().expect("the binary runs");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "`taiyaku {}` failed: {stdout}{}",
        args.join(" "),
        String::from_utf8_lossy(&run.stderr)
    );
    stdout
}

/// The arguments of the `taiyaku` command `message` names between backticks.
fn named_command(message: &str) -> Vec<String> {
    let command = message
        .split('`')
        .skip(1)
        .step_by(2)
        .find(|quoted| quoted.starts_with("taiyaku "))
        .unwrap_or_else(|| panic!("no `taiyaku …` command is named in: {message}"));
    command
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect()
}

/// A fixture sidecar, `taiyaku.json` in `dir`: one hub edition of one member.
/// `content_key` hashes the member's *declared* `sha256`, not its actual bytes,
/// so no archive needs to exist on disk.
fn fixture(dir: &Path) -> String {
    let path = dir.join("taiyaku.json");
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

/// A `--db` path, `library.db` in `dir`.
fn db_path(dir: &Path) -> String {
    let path = dir.join("library.db");
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

#[test]
fn the_usage_advertises_serve_dirs() {
    let run = taiyaku().output().expect("the binary runs");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let serve_line = stdout
        .lines()
        .find(|l| l.contains("taiyaku serve"))
        .expect("the usage lists serve");
    assert!(
        serve_line.contains("[--dirs DIR]..."),
        "serve's usage line advertises the flag: {serve_line}"
    );
}

/// The usage lists `library scan` and `library rules`, so a person running
/// bare `taiyaku` can discover them.
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
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("taiyaku library libraries")),
        "the usage lists library libraries: {stdout}"
    );
    let scan_line = stdout
        .lines()
        .find(|l| l.contains("taiyaku library scan"))
        .expect("the usage lists library scan");
    assert!(
        scan_line.contains("[--library NAME | --root PATH]"),
        "library scan's usage line advertises the flag: {scan_line}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("taiyaku library user grant-library|revoke-library")),
        "the usage lists the two grants: {stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("taiyaku library user every-library <user>")),
        "the usage lists the command that lifts a limit: {stdout}"
    );
}

/// The directory `name` under `tree`, one file in it, added to the library at
/// `db` with `scan --add --no-derive`, under `--library` where one is given.
/// Returns its canonical path and what the command printed to stdout.
fn add_dir(tree: &Path, db: &str, name: &str, library: Option<&str>) -> (String, String) {
    let dir = tree.join(name);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("placeholder.txt"), b"x").expect("a file, so the root is not empty");
    let mut args = vec!["library", "scan", "--add", dir.to_str().unwrap()];
    if let Some(library) = library {
        args.extend(["--library", library]);
    }
    args.extend(["--no-derive", "--db", db]);
    let printed = taiyaku_stdout(&args);
    let canonical = dir
        .canonicalize()
        .expect("canonical")
        .to_str()
        .expect("a utf-8 temp path")
        .to_string();
    (canonical, printed)
}

/// The directories `Comics`, `Volumes` and `Chapters` under `tree`, one file in
/// each, added to the library at `db` in that order. `Comics` is a library of
/// its own and the other two are the library `Manga`. Returns their canonical
/// paths in the same order.
fn comics_and_a_two_dir_manga(tree: &Path, db: &str) -> [String; 3] {
    let dirs = [
        ("Comics", None),
        ("Volumes", Some("Manga")),
        ("Chapters", Some("Manga")),
    ];
    dirs.map(|(name, library)| add_dir(tree, db, name, library).0)
}

/// `scan --add PATH --library NAME` walks `NAME`'s directories, the new one among
/// them, and no directory of another library.
#[test]
fn library_scan_add_with_a_library_walks_only_that_library() {
    let tree = TempDir::new("cli-scan-add-library");
    let db_dir = TempDir::new("cli-scan-add-library-db");
    let db = db_path(&db_dir);
    let (comics, _) = add_dir(&tree, &db, "Comics", None);
    let (volumes, _) = add_dir(&tree, &db, "Volumes", Some("Manga"));

    let (chapters, printed) = add_dir(&tree, &db, "Chapters", Some("Manga"));

    let reported = |dir: &str| printed.contains(&format!("root {dir} "));
    assert_eq!(
        [reported(&comics), reported(&volumes), reported(&chapters)],
        [false, true, true],
        "{printed}"
    );
}

/// `scan --add` says first which library the directory is now in, by the name
/// the store holds: the directory's own name, its whole path where a library
/// already has that name, and the library's name where `--library` was typed
/// with spaces around it. A directory added without `--library` made its
/// library, and the line says so.
#[test]
fn library_scan_add_says_which_library_the_dir_is_filed_under() {
    let tree = TempDir::new("cli-scan-add-filed");
    let db_dir = TempDir::new("cli-scan-add-filed-db");
    let db = db_path(&db_dir);

    let (_, own) = add_dir(&tree, &db, "one/Manga", None);
    let (second, by_path) = add_dir(&tree, &db, "two/Manga", None);
    let (_, joined) = add_dir(&tree, &db, "shelf", Some(" Manga "));

    let first_line = |printed: &str| printed.lines().next().unwrap_or_default().to_string();
    assert_eq!(
        [first_line(&own), first_line(&by_path), first_line(&joined)],
        [
            "created the library `Manga` for this directory".to_string(),
            format!("created the library `{second}` for this directory"),
            "filed under the library `Manga`".to_string(),
        ]
    );
}

/// `scan --add PATH --library NAME` says whether it made the library: the
/// first directory added under a name no library answers to creates it, and the
/// next joins it.
#[test]
fn library_scan_add_with_a_library_says_whether_it_made_the_library() {
    let tree = TempDir::new("cli-scan-add-created");
    let db_dir = TempDir::new("cli-scan-add-created-db");
    let db = db_path(&db_dir);

    let (_, first) = add_dir(&tree, &db, "Volumes", Some("New"));
    let (_, second) = add_dir(&tree, &db, "Chapters", Some("New"));

    let first_line = |printed: &str| printed.lines().next().unwrap_or_default().to_string();
    assert_eq!(
        [first_line(&first), first_line(&second)],
        [
            "created the library `New` for this directory",
            "filed under the library `New`",
        ]
    );
}

/// `scan --library` derives once its directories are walked, and derives the
/// whole store: a directory of another library, walked before with
/// `--no-derive`, gains its work.
#[test]
fn library_scan_with_a_library_derives_the_whole_store() {
    let tree = TempDir::new("cli-scan-library-derives");
    let db_dir = TempDir::new("cli-scan-library-derives-db");
    let db = db_path(&db_dir);
    let mut dirs = Vec::new();
    for (name, series, library) in [
        ("Comics", "Saga", None),
        ("Volumes", "Kaguya", Some("Manga")),
    ] {
        let dir = tree.join(name);
        let volume = dir.join(series).join(format!("{series} - v01"));
        std::fs::create_dir_all(&volume).expect("mkdir");
        std::fs::write(volume.join("000.jpg"), b"page").expect("write");
        let mut args = vec!["library", "scan", "--add", dir.to_str().unwrap()];
        if let Some(library) = library {
            args.extend(["--library", library]);
        }
        args.extend(["--no-derive", "--db", &db]);
        taiyaku_stdout(&args);
        dirs.push(dir.canonicalize().expect("canonical").display().to_string());
    }
    let listing = ["library", "libraries", "--db", &db];
    let listed = |works: usize| {
        format!(
            "Comics: 1 directory, {works} work(s)\n  {}\n\
             Manga: 1 directory, {works} work(s)\n  {}\n",
            dirs[0], dirs[1]
        )
    };
    let before = taiyaku_stdout(&listing);

    let scanned = taiyaku_stdout(&["library", "scan", "--library", "Manga", "--db", &db]);

    let derived = scanned
        .lines()
        .any(|line| line.starts_with("derived 2 series, 2 works, "));
    assert_eq!(
        (before, derived, taiyaku_stdout(&listing)),
        (listed(0), true, listed(1)),
        "{scanned}"
    );
}

#[test]
fn library_libraries_lists_each_library_with_its_dirs() {
    let tree = TempDir::new("cli-libraries");
    let db_dir = TempDir::new("cli-libraries-db");
    let db = db_path(&db_dir);
    let [comics, volumes, chapters] = comics_and_a_two_dir_manga(&tree, &db);

    let listed = taiyaku_stdout(&["library", "libraries", "--db", &db]);

    assert_eq!(
        listed,
        format!(
            "Comics: 1 directory, 0 work(s)\n  {comics}\n\
             Manga: 2 directories, 0 work(s)\n  {volumes}\n  {chapters}\n"
        )
    );
}

/// With no library the listing says so and names the command that makes one,
/// and that command, run as the listing spells it, makes one. A work imported
/// from a `taiyaku.json` is counted as in no library before and after.
#[test]
fn library_libraries_says_there_are_none_and_counts_the_works_in_none() {
    let tree = TempDir::new("cli-libraries-none");
    let dir = tree.join("Shelf");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("placeholder.txt"), b"x").expect("write");
    let shelf = dir.canonicalize().expect("canonical");
    let db_dir = TempDir::new("cli-libraries-none-db");
    let db = db_path(&db_dir);
    let listing = ["library", "libraries", "--db", &db];
    let none = "no libraries yet; `taiyaku library scan --add PATH` adds a directory as one\n";
    let unfiled = "1 work(s) in no library\n";

    let empty = taiyaku_stdout(&listing);
    assert_eq!(empty, none);

    taiyaku_stdout(&["library", "import", &fixture(&db_dir), "--db", &db]);
    assert_eq!(taiyaku_stdout(&listing), format!("{none}{unfiled}"));

    let mut add: Vec<String> = named_command(&empty)
        .into_iter()
        .map(|word| {
            if word == "PATH" {
                dir.to_str().unwrap().to_string()
            } else {
                word
            }
        })
        .collect();
    add.extend(["--db".to_string(), db.clone()]);
    taiyaku_stdout(&add.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        taiyaku_stdout(&listing),
        format!(
            "Shelf: 1 directory, 0 work(s)\n  {}\n{unfiled}",
            shelf.display()
        )
    );
}

/// `scan --library` walks the named library's directories, both of them, and no
/// directory of another library. Each directory gains a file after it was added, so
/// a directory that was walked has that file recorded and one that was not does
/// not.
#[test]
fn library_scan_with_a_library_walks_only_its_dirs() {
    let tree = TempDir::new("cli-scan-library");
    let db_dir = TempDir::new("cli-scan-library-db");
    let db = db_path(&db_dir);
    let [comics, volumes, chapters] = comics_and_a_two_dir_manga(&tree, &db);
    for dir in [&comics, &volumes, &chapters] {
        std::fs::write(Path::new(dir).join("new.txt"), b"x").expect("write");
    }

    let reported = taiyaku_stdout(&[
        "library",
        "scan",
        "--library",
        "Manga",
        "--no-derive",
        "--db",
        &db,
    ]);

    let conn = rusqlite::Connection::open(&db).expect("open the same database taiyaku wrote");
    // Whether the scan reported on `dir`, and the files recorded under it.
    let walked = |dir: &str| -> (bool, Vec<String>) {
        let mut files = conn
            .prepare(
                "SELECT e.rel_path FROM scan_entry e JOIN root r ON r.id = e.root_id \
                  WHERE r.path = ?1 AND e.kind = 'file' ORDER BY e.rel_path",
            )
            .expect("prepares");
        let recorded = files
            .query_map([dir], |row| row.get(0))
            .expect("reads")
            .collect::<Result<Vec<String>, _>>()
            .expect("reads");
        (reported.contains(&format!("root {dir} ")), recorded)
    };
    let before = vec!["placeholder.txt".to_string()];
    let both = vec!["new.txt".to_string(), "placeholder.txt".to_string()];
    assert_eq!(
        [walked(&comics), walked(&volumes), walked(&chapters)],
        [(false, before), (true, both.clone()), (true, both)],
        "{reported}"
    );
}

#[test]
fn library_scan_refuses_a_library_it_does_not_hold_and_root_with_library() {
    let tree = TempDir::new("cli-scan-library-refusals");
    let db_dir = TempDir::new("cli-scan-library-refusals-db");
    let db = db_path(&db_dir);
    let [_, volumes, _] = comics_and_a_two_dir_manga(&tree, &db);

    let run = taiyaku()
        .args(["library", "scan", "--library", "Nope", "--db", &db])
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    assert!(
        stderr.lines().any(|line| line
            == "taiyaku: no library named `Nope`; `taiyaku library libraries` lists them"),
        "{stderr}"
    );
    assert!(run.stdout.is_empty(), "nothing is walked");

    // A directory of `Manga` and `Manga` itself, so neither flag is wrong alone,
    // and a directory to add, which the refusal comes before.
    let unadded = tree.join("Unadded");
    std::fs::create_dir_all(&unadded).expect("mkdir");
    std::fs::write(unadded.join("placeholder.txt"), b"x").expect("write");
    let listing = ["library", "libraries", "--db", &db];
    let before = taiyaku_stdout(&listing);
    let run = taiyaku()
        .args([
            "library",
            "scan",
            "--add",
            unadded.to_str().unwrap(),
            "--root",
            &volumes,
            "--library",
            "Manga",
            "--db",
            &db,
        ])
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(
        (
            run.status.code(),
            String::from_utf8_lossy(&run.stdout).into_owned(),
            taiyaku_stdout(&listing)
        ),
        (Some(1), String::new(), before),
        "a failing exit, with nothing printed, walked or added: {stderr}"
    );
    assert!(
        stderr.lines().any(|line| line
            == "taiyaku: `--root` and `--library` each choose which directories are walked; \
                pass one of them"),
        "{stderr}"
    );
}

/// `grant-library` and `revoke-library` name the library as the store holds
/// it, where it was typed with spaces around it.
#[test]
fn library_user_grants_name_the_library_as_the_store_holds_it() {
    let tree = TempDir::new("cli-grant-stored-name");
    let db_dir = TempDir::new("cli-grant-stored-name-db");
    let db = db_path(&db_dir);
    add_dir(&tree, &db, "Volumes", Some("Manga"));
    {
        let library = taiyaku_library::Library::open(Path::new(&db)).expect("opens");
        taiyaku_library::account::promote(&library.conn(), "davey", "a password", "admin")
            .expect("promotes");
    }

    let first_line = |verb: &str| {
        taiyaku_stdout(&["library", "user", verb, "davey", " Manga ", "--db", &db])
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        [
            first_line("grant-library"),
            first_line("grant-library"),
            first_line("revoke-library"),
            first_line("revoke-library"),
        ],
        [
            "granted sight of Manga",
            "that account could already see Manga; nothing to do",
            "revoked sight of Manga",
            "that account had no grant for Manga; nothing to revoke",
        ]
    );
}

/// What `grant-library` and `revoke-library` say under their first line. A
/// grant to a reader that saw every library says it narrows the account, and
/// the same grant made again does not. Revoking it says the account now sees
/// no library, and names the command that lifts the limit. An administrator
/// saw every library throughout, so revoking its grant says that nothing
/// changes, and revoking nothing says nothing more.
#[test]
fn library_user_grant_notes_follow_the_role_and_whether_anything_changed() {
    let tree = TempDir::new("cli-grant-notes");
    let db_dir = TempDir::new("cli-grant-notes-db");
    let db = db_path(&db_dir);
    add_dir(&tree, &db, "Volumes", Some("Manga"));
    {
        let library = taiyaku_library::Library::open(Path::new(&db)).expect("opens");
        let conn = library.conn();
        taiyaku_library::account::promote(&conn, "davey", "a password", "admin").expect("promotes");
        taiyaku_library::account::add(&conn, "alex", "theirs", "user").expect("adds a reader");
    }
    let said = |verb: &str, user: &str| {
        taiyaku_stdout(&["library", "user", verb, user, "Manga", "--db", &db])
    };

    let reader = [
        said("grant-library", "alex"),
        said("grant-library", "alex"),
        said("revoke-library", "alex"),
    ];
    said("grant-library", "davey");
    let administrator = [
        said("revoke-library", "davey"),
        said("revoke-library", "davey"),
    ];

    assert_eq!(
        reader,
        [
            "granted sight of Manga\n  That account saw every library, and is now restricted \
             to that one. `taiyaku library user every-library alex` lifts the limit.\n",
            "that account could already see Manga; nothing to do\n",
            "revoked sight of Manga\n  `alex` now sees no library; `taiyaku library user \
             every-library alex` lifts the limit.\n",
        ]
    );
    assert_eq!(
        administrator,
        [
            "revoked sight of Manga\n  That account is an administrator, so this revocation \
             changes nothing while it stays one — an administrator always sees every library.\n",
            "that account had no grant for Manga; nothing to revoke\n",
        ]
    );
}

/// `user list` shows what each account sees: the libraries it is granted, by
/// name, every library, or no library once its last grant is revoked. An
/// administrator holding no grant has nothing added to its line. A second
/// library granted to an account already limited is reported as granted: the
/// limit does not move, but the grant is new.
#[test]
fn library_user_list_shows_what_each_account_is_granted() {
    let tree = TempDir::new("cli-user-list-grants");
    let db_dir = TempDir::new("cli-user-list-grants-db");
    let db = db_path(&db_dir);
    add_dir(&tree, &db, "Volumes", Some("Other"));
    add_dir(&tree, &db, "Comics", Some("Manga"));
    {
        let library = taiyaku_library::Library::open(Path::new(&db)).expect("opens");
        let conn = library.conn();
        taiyaku_library::account::promote(&conn, "davey", "a password", "admin").expect("promotes");
        taiyaku_library::account::add(&conn, "alex", "theirs", "user").expect("adds a reader");
        taiyaku_library::account::add(&conn, "sam", "theirs", "user").expect("adds another");
        taiyaku_library::account::add(&conn, "kim", "theirs", "user").expect("adds a third");
        taiyaku_library::account::add(&conn, "pat", "theirs", "user").expect("adds a fourth");
    }
    let said: Vec<String> = [
        ("grant-library", "alex", "Other"),
        ("grant-library", "alex", "Manga"),
        ("grant-library", "sam", "Other"),
        ("grant-library", "pat", "Manga"),
        ("revoke-library", "pat", "Manga"),
    ]
    .into_iter()
    .map(|(verb, user, library)| {
        taiyaku_stdout(&["library", "user", verb, user, library, "--db", &db])
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    })
    .collect();

    let listed = taiyaku_stdout(&["library", "user", "list", "--db", &db]);

    assert_eq!(
        said,
        [
            "granted sight of Other",
            "granted sight of Manga",
            "granted sight of Other",
            "granted sight of Manga",
            "revoked sight of Manga",
        ]
    );
    assert_eq!(
        listed,
        "1 davey (admin)\n\
         2 alex (user) — granted: Manga, Other\n\
         3 sam (user) — granted: Other\n\
         4 kim (user) — every library\n\
         5 pat (user) — no library\n"
    );
}

/// `every-library` lifts an account's limit, deleting its grants, and says
/// so; asked again it says nothing changed. An account left seeing no library
/// by its last revocation is lifted the same way.
#[test]
fn library_user_every_library_lifts_the_limit_and_says_whether_it_did() {
    let tree = TempDir::new("cli-every-library");
    let db_dir = TempDir::new("cli-every-library-db");
    let db = db_path(&db_dir);
    add_dir(&tree, &db, "Volumes", Some("Manga"));
    {
        let library = taiyaku_library::Library::open(Path::new(&db)).expect("opens");
        let conn = library.conn();
        taiyaku_library::account::promote(&conn, "davey", "a password", "admin").expect("promotes");
        taiyaku_library::account::add(&conn, "alex", "theirs", "user").expect("adds a reader");
        taiyaku_library::account::add(&conn, "pat", "theirs", "user").expect("adds another");
    }
    let run = |args: &[&str]| {
        let mut all = vec!["library", "user"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["--db", &db]);
        taiyaku_stdout(&all)
    };
    run(&["grant-library", "alex", "Manga"]);
    run(&["grant-library", "pat", "Manga"]);
    run(&["revoke-library", "pat", "Manga"]);

    let said = [
        run(&["every-library", "alex"]),
        run(&["every-library", "alex"]),
        run(&["every-library", "pat"]),
    ];

    assert_eq!(
        said,
        [
            "`alex` now sees every library\n",
            "`alex` already sees every library; nothing to do\n",
            "`pat` now sees every library\n",
        ]
    );
    let library = taiyaku_library::Library::open(Path::new(&db)).expect("opens");
    let conn = library.conn();
    for user in ["alex", "pat"] {
        let id = taiyaku_library::account::id_of(&conn, user)
            .expect("looks up")
            .expect("the account");
        assert_eq!(
            taiyaku_library::account::access_for(&conn, id).expect("reads"),
            taiyaku_library::account::Access::Every,
            "{user}"
        );
        assert!(
            taiyaku_library::account::libraries_for(&conn, id)
                .expect("reads")
                .is_empty(),
            "{user}: its grants went"
        );
    }
}

/// `grant-library`, `revoke-library` and `scan --library` answer a library
/// they cannot find, and `grant-root` and `revoke-root` answer at all, with a
/// command that lists the libraries. This runs the command each one names, as
/// it names it, and it lists them.
#[test]
fn the_command_a_missing_library_is_answered_with_lists_the_libraries() {
    let tree = TempDir::new("cli-missing-library");
    let db_dir = TempDir::new("cli-missing-library-db");
    let db = db_path(&db_dir);
    comics_and_a_two_dir_manga(&tree, &db);
    {
        // `grant-library` and `revoke-library` look the account up first.
        let library = taiyaku_library::Library::open(Path::new(&db)).expect("opens");
        taiyaku_library::account::promote(&library.conn(), "davey", "a password", "admin")
            .expect("promotes");
    }

    let refused: [&[&str]; 5] = [
        &["library", "user", "grant-library", "davey", "Nope"],
        &["library", "user", "revoke-library", "davey", "Nope"],
        &["library", "user", "grant-root", "davey", "Nope"],
        &["library", "user", "revoke-root", "davey", "Nope"],
        &["library", "scan", "--library", "Nope"],
    ];
    for args in refused {
        let run = taiyaku()
            .args(args)
            .args(["--db", &db])
            .output()
            .expect("the binary runs");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert_eq!(run.status.code(), Some(1), "{args:?}: {stderr}");
        let message = stderr
            .lines()
            .find(|line| line.contains("lists them"))
            .unwrap_or_else(|| panic!("{args:?} names no listing: {stderr}"));

        let mut listing = named_command(message);
        listing.extend(["--db".to_string(), db.clone()]);
        let listed = taiyaku_stdout(&listing.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(
            listed.contains("Manga: 2 directories, 0 work(s)\n"),
            "{args:?} names `taiyaku {}`, which prints: {listed}",
            listing.join(" ")
        );
    }
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
    let scratch = TempDir::new("cli-import-reports-fixture");
    let db_dir = TempDir::new("cli-import-reports");
    let out = taiyaku_output(&[
        "library",
        "import",
        &fixture(&scratch),
        "--db",
        &db_path(&db_dir),
    ]);
    assert!(out.contains("imported"), "{out}");
}

/// `taiyaku serve <sidecar>...` keeps working: it imports those paths and
/// serves them (design §5). That is the migration path for an existing install
/// and what keeps the corpus-gated tests in `serve/src/corpus.rs` alive — this
/// checks the argument *shape* survived (still a positional `<taiyaku.json>...`
/// list, not e.g. a `--sidecar` flag) without actually binding a port, which is
/// what `--help`'s early return (`main.rs`'s `run`) buys: the same whole-binary
/// usage `taiyaku` with no arguments prints.
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
/// content hash regardless of how its path was spelled. What this test
/// actually pins: two `taiyaku library import` calls against the identical
/// underlying file, spelled two different ways, leave exactly one `work`
/// row — reconciliation is by content, not by path string equality.
///
/// The placeholder hazard is specifically about a row whose `content_key` is
/// the `bridge:{slug}` sentinel — minted by `taiyaku serve` recording a reading
/// position *before* any import — which can never match on `content_key` and so
/// falls through to the `sidecar_path` comparison that `library.rs`'s
/// canonicalisation exists to fix. That is not reachable from this test,
/// because nothing here ever creates a placeholder row. The discriminating
/// coverage for that hazard lives in
/// `crates/taiyaku-cli/src/library.rs`'s own tests (`crates/taiyaku-cli/src/library/tests.rs`):
/// `importing_the_same_file_under_a_different_spelling_reuses_the_placeholder_row`
/// goes through `canonical_sidecar_path`/`import_canonical` and fails if
/// `canonical_sidecar_path` is reverted to a no-op;
/// `without_canonicalisation_the_same_scenario_would_orphan_the_placeholder`
/// calls `taiyaku_library::import::sidecar` directly, bypassing
/// `canonical_sidecar_path` on purpose, to show the underlying defect is real
/// rather than hypothetical (it passes regardless of the fix, since it never
/// invokes the fixed function at all — that is by design, not a gap).
#[test]
fn two_imports_of_one_file_reconcile_by_content_not_path() {
    let dir = TempDir::new("cli-reimport-spelling");
    let sidecar = dir.join("taiyaku.json");
    std::fs::write(
        &sidecar,
        r#"{"taiyaku":1,"work":{"title":"Reimport Spelling","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"cd34","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    let db_dir = TempDir::new("cli-reimport-spelling");
    let db = db_path(&db_dir);

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

/// **`export_targets`' stdout reporting surface — `wrote`, `removed`, `kept`
/// and the final summary line — as a reader running the real binary sees it.**
/// The *arithmetic* behind the totals is pinned by
/// `export_all_into_one_dir_keeps_every_work_rather_than_overwriting`, a unit
/// test over `export_targets` directly. This drives `taiyaku library import`
/// and `taiyaku library export --in-place` and reads every one of the four
/// kinds of line off the process's actual stdout.
///
/// The recipe behind each outcome:
/// - **`wrote`**: `taiyaku.json` is written on every in-place export
///   unconditionally (C2's refusal exists precisely because this always
///   happens), and `taiyaku.notes.json` is always written too — nothing has
///   ever recorded notes for this fixture, so there is nothing to keep.
/// - **`removed`**: a hand-written `taiyaku.library.json` holding nothing but
///   `taiyaku_library`/`updated` — the content-free "skeleton" shape
///   `export::work`'s own doc comment describes — migrates cleanly on import
///   (`library=migrated`, and the marker records it absorbed) but stays an
///   *empty* `LibraryDoc`, so the next export finds no state behind it and
///   deletes it.
/// - **`kept`**: a `taiyaku.truth.json` that appears only *after* the first
///   import is reported `truth=unread` on the second import — settled, but
///   never read — and the export that follows leaves it alone rather than
///   deleting or overwriting it. The same recipe as the unit test
///   `export_in_place_does_not_delete_a_sibling_the_database_never_absorbed`,
///   one layer up, through the real binary's own stdout.
#[test]
fn export_reports_wrote_removed_kept_and_the_summary_line_on_real_stdout() {
    let dir = TempDir::new("cli-export-reporting");
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

    let db_dir = TempDir::new("cli-export-reporting");
    let db = db_path(&db_dir);

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

/// **`library.rs`'s call site of `malformed_sibling_warnings`.** The
/// `for line in malformed_sibling_warnings(...) { eprintln!("{line}"); }` loop
/// in `library.rs::import` is what tells a reader a sibling document was left
/// unread. `malformed_sibling_warnings` itself is tested in `library/tests.rs`
/// (`every_malformed_sibling_gets_a_sentence_naming_the_file_and_the_reason`).
/// This drives `taiyaku library import` against a fixture with an unreadable
/// `taiyaku.notes.json` and reads the warning off the binary's real stderr.
#[test]
fn library_import_prints_the_malformed_sibling_warning_to_stderr() {
    let dir = TempDir::new("cli-import-malformed-warning");
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

    let db_dir = TempDir::new("cli-import-malformed-warning");
    let out = taiyaku_output(&[
        "library",
        "import",
        sidecar.to_str().unwrap(),
        "--db",
        &db_path(&db_dir),
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

/// `derive` calls `proposals::raise` once, at the end of its own pass; the
/// count `library scan` prints must be that one.
/// `derive_summary_prints_suppressed_and_rekeyed_when_nonzero` exercises the
/// pure formatter with a hand-built `raised` argument and never the real
/// command's own count, so this runs `library scan` for real and reads its
/// real stdout.
///
/// A tree with a genuine colour-edition suffix — the same shape
/// `proposals::tests::library_with_two_suffixed_series` scans — is what
/// raises exactly one `edition-of` proposal.
#[test]
fn library_scan_reports_the_real_proposal_count_on_real_stdout() {
    let dir = TempDir::new("cli-scan-proposals");
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

    let db_dir = TempDir::new("cli-scan-proposals");
    let out = taiyaku_output(&[
        "library",
        "scan",
        "--add",
        dir.to_str().unwrap(),
        "--db",
        &db_path(&db_dir),
    ]);

    assert!(
        out.contains("; 1 proposal(s) raised"),
        "the real command must report the one real proposal this tree raises, not zero: {out}"
    );

    // §8's per-root watch report and the walk's own added/updated/gone line
    // are two separate `println!`s in `scan`, and `scan_summary`'s unit tests
    // exercise the formatter with a hand-built report, never the command. A
    // counter nobody prints is the failure it exists to prevent — the rule
    // `derive_summary`'s own conditional fields follow — so this reads both
    // lines where a person actually reads them.
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

/// **I3, end to end.** This drives the real binary through a scan that raises
/// one proposal, lists it, and accepts it — reading everything off the
/// process's actual stdout, the way
/// `library_import_reports_what_the_merge_did` does for `import`.
#[test]
fn library_proposals_lists_and_accepts_over_real_stdout() {
    let dir = TempDir::new("cli-proposals");
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
    let db_dir = TempDir::new("cli-proposals-e2e");
    let db = db_path(&db_dir);

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
/// Pinned at the binary (O70): `proposals::unaccept` and `proposals::dismiss`
/// answer a refusal as `Ok(Offer::Refused(why))`, so this command turns it
/// into the `Err(String)` that `main` prints as `taiyaku: {message}` before
/// exiting 1. An arm that printed the success line instead, or reworded the
/// sentence on the way, is a changed exit code or a changed message, and the
/// exact line is what tells.
#[test]
fn library_proposals_refusals_exit_one_with_the_librarys_own_sentence() {
    let dir = TempDir::new("cli-proposals-refusals");
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
    let db_dir = TempDir::new("cli-proposals-refusals");
    let db = db_path(&db_dir);

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

    // And accepted, so a reopen (O72) is refused too, pointing at un-accepting.
    let run = taiyaku()
        .args(["library", "proposals", "--reopen", id, "--db", &db])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let expected = format!(
        "taiyaku: proposal {id} is accepted, not dismissed — un-accepting it is what reopens it"
    );
    assert!(
        stderr.lines().any(|line| line == expected),
        "the library's sentence, unreworded, on its own line: {stderr}"
    );
    assert!(!stdout.contains("reopened proposal"), "{stdout}");
}

/// **`if !reopened.is_empty()`, not `if reopened.is_empty()`.** An import
/// that orphans an accepted proposal's rule must say so; `library.rs`'s own
/// unit tests call `rules` directly and this crate has no stdout-capturing
/// helper (deliberately, per the note below), so the only place this line
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
    let dir = TempDir::new("cli-rules-reopen");
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
    let db_dir = TempDir::new("cli-rules-import-reopen");
    let db = db_path(&db_dir);

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

    // Stdout alone, like the note test below: mixing in stderr's
    // "library database at …" line would break the JSON this is fed straight
    // back into.
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
    let path = db_dir.join("rules.json");
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

/// **`main.rs`'s call site of `malformed_sibling_warnings`.** The identical
/// loop is duplicated in `serve_work`: `taiyaku serve <sidecar>...` imports
/// every path it is given the same way `library import` does. `serve` never
/// returns on success, so this occupies the address first with a listener of
/// its own: the bind inside `taiyaku_serve::serve` then fails immediately with
/// `AddrInUse`, the process exits instead of blocking forever, and everything
/// printed *before* that bind attempt — including this warning, which fires
/// while importing, well before the network is ever touched — is still on its
/// stderr to read.
#[test]
fn serve_prints_the_malformed_sibling_warning_before_refusing_to_bind() {
    let dir = TempDir::new("cli-serve-malformed-warning");
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

    let db_dir = TempDir::new("cli-serve-malformed-warning");
    let run = taiyaku()
        .args([
            "serve",
            sidecar.to_str().unwrap(),
            "--db",
            &db_path(&db_dir),
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

/// **A rule import stores rules and derives nothing**, so the command has to
/// name the scan that applies them. The whole claim the rule layer makes is
/// that editing a rule re-derives the library without touching the filesystem
/// — which is true, and requires something to call `derive`. A person who has
/// just replaced every rule and seen only a success line would reasonably
/// believe the library already reflects them.
///
/// Driven through the real binary because that is where the line reaches
/// anybody: `library.rs`'s own unit tests call `rules` directly and this
/// crate has no stdout-capturing helper, deliberately.
#[test]
fn library_rules_import_says_a_scan_is_needed_before_the_rules_decide_anything() {
    let db_dir = TempDir::new("cli-rules-import-note");
    let db = db_path(&db_dir);
    // Stdout alone here, not `taiyaku_output`: this document is fed straight
    // back to `--import`, and the "library database at …" note the command
    // writes to stderr would not parse as JSON beside it.
    let run = taiyaku()
        .args(["library", "rules", "--db", &db])
        .output()
        .expect("the binary runs");
    let exported = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(exported.contains("\"ordinal\""), "{exported}");

    let path = db_dir.join("rules.json");
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
    let db_dir = TempDir::new("cli-scan-positional");
    let run = taiyaku()
        .args(["library", "scan", "/srv/manga", "--db", &db_path(&db_dir)])
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

/// A value in `TAIYAKU_TRUSTED_PROXY` that is not an address range is refused
/// before anything binds, and the message names the variable: nothing on the
/// command line was wrong, so a message naming only the value would send a
/// reader hunting through their flags.
///
/// The variable is set on the child's environment and not on this process's,
/// which other tests in the binary share.
#[test]
fn serve_refuses_a_malformed_trusted_proxy_variable_by_name() {
    let dir = TempDir::new("cli-serve-proxy-env");
    let sidecar = fixture(&dir);
    let db_dir = TempDir::new("cli-serve-proxy-env-db");
    let mut child = ServeChild(
        taiyaku()
            .env("TAIYAKU_TRUSTED_PROXY", "nonsense")
            .args([
                "serve",
                &sidecar,
                "--db",
                &db_path(&db_dir),
                "--addr",
                "127.0.0.1:0",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the binary runs"),
    );

    // Polled against a deadline: a server that did not refuse would run
    // forever, and that must fail this test rather than hang it. The deadline
    // stays below the runner's own kill so that this message is the one printed.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("polls the child") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            panic!("the server started with a malformed TAIYAKU_TRUSTED_PROXY");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().expect("piped"), &mut stderr)
        .expect("reads stderr");

    assert_eq!(status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("TAIYAKU_TRUSTED_PROXY"), "{stderr}");
    assert!(stderr.contains("nonsense"), "{stderr}");
}

/// The running server refuses the old `TAIYAKU_FOLDERS` before it listens,
/// naming the variable to set instead. A valid `--dirs` is given beside it, so
/// the old variable is the only thing wrong and the flag does not excuse it.
///
/// The variable is set on the child's environment and not on this process's,
/// which other tests in the binary share.
#[test]
fn serve_refuses_the_old_dirs_variable_before_it_listens() {
    let dir = TempDir::new("cli-serve-old-dirs-variable");
    let db_dir = TempDir::new("cli-serve-old-dirs-variable-db");
    let from_flag = dir.join("from-flag");
    std::fs::create_dir_all(&from_flag).expect("mkdir");
    let mut child = ServeChild(
        taiyaku()
            .env("TAIYAKU_FOLDERS", dir.join("from-variable"))
            .args([
                "serve",
                "--db",
                &db_path(&db_dir),
                "--addr",
                "127.0.0.1:0",
                "--dirs",
            ])
            .arg(&from_flag)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the binary runs"),
    );

    // Polled against a deadline for the reason the malformed-proxy test gives.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("polls the child") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            panic!("the server started with TAIYAKU_FOLDERS set");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().expect("piped"), &mut stderr)
        .expect("reads stderr");

    assert_eq!(status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("TAIYAKU_FOLDERS"), "{stderr}");
    assert!(stderr.contains("TAIYAKU_DIRS"), "{stderr}");
    assert!(!stderr.contains("listening"), "{stderr}");
}

/// A directory the server was started with that is not there is named on its
/// console before it says it is listening, so a deployer whose share is not
/// mapped sees why the browser is empty. A `TAIYAKU_DIRS` is set on the
/// child's environment (never this process's) and a `--dirs` given beside
/// it: the flag replaces the variable, so only the flag's directory is in use.
#[cfg(unix)]
#[test]
fn serve_says_which_dirs_it_cannot_browse() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let dir = TempDir::new("cli-serve-dirs");
    let db_dir = TempDir::new("cli-serve-dirs-db");
    let cache = TempDir::new("cli-serve-dirs-cache");
    let from_variable = dir.join("unmapped-variable");
    let from_flag = dir.join("unmapped-flag");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().expect("reads the port").to_string();
    drop(probe);

    let mut child = ServeChild(
        taiyaku()
            .env("TAIYAKU_DIRS", &from_variable)
            .args([
                "serve",
                "--db",
                &db_path(&db_dir),
                "--cache",
                cache.to_str().expect("a utf-8 temp path"),
                "--addr",
                &addr,
                "--dirs",
            ])
            .arg(&from_flag)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs"),
    );
    let stderr = child.stderr.take().expect("stderr is piped");
    let (said, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if said.send(line).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut seen = Vec::new();
    loop {
        let left = Duration::from_secs(20).saturating_sub(started.elapsed());
        match heard.recv_timeout(left) {
            Ok(line) => {
                let listening = line.contains("listening");
                seen.push(line);
                if listening {
                    break;
                }
            }
            Err(_) => panic!("the server never said it was listening: {seen:?}"),
        }
    }

    let said_of = |path: &std::path::Path| {
        seen.iter()
            .filter(|line| line.contains(&path.display().to_string()))
            .count()
    };
    assert_eq!(said_of(&from_flag), 1, "{seen:?}");
    assert_eq!(
        said_of(&from_variable),
        0,
        "a flag replaces the variable, so the variable's directory is not in use: {seen:?}"
    );
}

/// A spawned `taiyaku serve`, killed and reaped when it goes out of scope.
///
/// A test that panics between the spawn and its own cleanup (a `kill` that did
/// not run, a poll that failed) would otherwise leave a server running, which
/// the test runner reports as a leak and which holds its port.
struct ServeChild(std::process::Child);

impl std::ops::Deref for ServeChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ServeChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for ServeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A `taiyaku serve` that has said it is listening, and what it said.
///
/// Field order matters: the child is dropped, so killed, before the
/// directories it uses are removed.
#[cfg(unix)]
struct Serving {
    child: ServeChild,
    addr: String,
    /// The `--db` it was started with.
    db: String,
    seen: Vec<String>,
    /// Held so the reader thread keeps draining stderr when `keep_reading` is
    /// set: a pipe nobody reads any more makes the server's next `eprintln!`
    /// panic.
    _heard: std::sync::mpsc::Receiver<String>,
    _dirs: [TempDir; 3],
}

/// Starts `taiyaku serve` on a free local port and waits for its `listening`
/// line.
///
/// The wait is bounded below the runner's own kill, so a server that never says
/// it is listening fails with its message instead of being reported as killed.
#[cfg(unix)]
fn serve_until_listening(label: &str) -> Serving {
    serve_listening(label, true)
}

/// [`serve_until_listening`], optionally letting go of the child's stderr
/// once the `listening` line is read, so that the server's next write to it
/// fails as it does under `serve … 2>&1 | tee log` after Ctrl-C.
#[cfg(unix)]
fn serve_listening(label: &str, keep_reading: bool) -> Serving {
    serve_listening_over(label, keep_reading, false)
}

/// [`serve_listening`], serving the fixture sidecar or, with `whole_library`,
/// no paths at all: every work in the library, read again whenever the
/// library moves.
#[cfg(unix)]
fn serve_listening_over(label: &str, keep_reading: bool, whole_library: bool) -> Serving {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let dir = TempDir::new(&format!("cli-serve-{label}"));
    let sidecar = fixture(&dir);
    let db_dir = TempDir::new(&format!("cli-serve-{label}-db"));
    let db = db_path(&db_dir);
    let cache = TempDir::new(&format!("cli-serve-{label}-cache"));
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().expect("reads the port").to_string();
    drop(probe);

    let mut args = vec!["serve"];
    if !whole_library {
        args.push(&sidecar);
    }
    args.extend([
        "--db",
        &db,
        "--cache",
        cache.to_str().expect("a utf-8 temp path"),
        "--addr",
        &addr,
    ]);
    let mut child = ServeChild(
        taiyaku()
            .args(&args)
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs"),
    );

    // Read on a thread so a server that never says it is listening fails
    // this test at the deadline instead of blocking it on a read forever.
    let stderr = child.stderr.take().expect("stderr is piped");
    let (said, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let listening = line.contains("listening");
            if said.send(line).is_err() || (listening && !keep_reading) {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut seen = Vec::new();
    loop {
        let left = Duration::from_secs(20).saturating_sub(started.elapsed());
        match heard.recv_timeout(left) {
            Ok(line) => {
                let listening = line.contains("listening");
                seen.push(line);
                if listening {
                    break;
                }
            }
            Err(_) => panic!("the server never said it was listening: {seen:?}"),
        }
    }
    Serving {
        child,
        addr,
        db,
        seen,
        _heard: heard,
        _dirs: [dir, db_dir, cache],
    }
}

/// Sends SIGTERM to the child, as `docker stop` does to PID 1.
#[cfg(unix)]
fn send_sigterm(child: &ServeChild) {
    let sent = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success(), "kill -TERM failed: {sent:?}");
}

/// Waits up to `limit` for the child to exit and returns its status; `None` if
/// it is still running.
#[cfg(unix)]
fn exit_within(
    child: &mut ServeChild,
    limit: std::time::Duration,
) -> Option<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().expect("polls the child") {
            return Some(status);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// `docker stop` sends SIGTERM to PID 1, and `taiyaku` is PID 1 in the image.
/// A process with no handler for it ignores it there, so the stop waits out its
/// grace period and ends in SIGKILL. Here the signal arrives the same way, and
/// the server must leave on its own with a success status.
///
/// The deadline is long on purpose: a loaded runner makes a slow exit, not a
/// wrong one, and the test only has to tell "exits" from "never exits". It
/// stays below the runner's own kill, so that its message is the one printed.
#[cfg(unix)]
#[test]
fn serve_exits_cleanly_on_sigterm() {
    let mut serving = serve_until_listening("sigterm");
    send_sigterm(&serving.child);

    let status = exit_within(&mut serving.child, std::time::Duration::from_secs(20))
        .unwrap_or_else(|| panic!("the server ignored SIGTERM: {:?}", serving.seen));
    assert!(
        status.success(),
        "{status:?} after SIGTERM: {:?}",
        serving.seen
    );
}

/// A client that has sent half a request head and goes quiet must not keep the
/// server from stopping. Graceful shutdown waits for open connections, and this
/// one never finishes; a download crawling to a slow phone holds it the same
/// way. Under `docker stop` the process would be killed at the timeout anyway,
/// but outside a container Ctrl-C would hang.
#[cfg(unix)]
#[test]
fn serve_exits_on_sigterm_with_a_connection_left_open() {
    use std::io::Write as _;
    use std::time::Duration;

    let mut serving = serve_until_listening("sigterm-open");
    let mut stuck = std::net::TcpStream::connect(&serving.addr).expect("connects");
    stuck
        .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n")
        .expect("sends half a request head");
    // Let the server accept it before the signal, so the connection is open
    // when shutdown begins.
    std::thread::sleep(Duration::from_millis(500));

    send_sigterm(&serving.child);

    let limit = taiyaku_serve::SHUTDOWN_GRACE + Duration::from_secs(5);
    let status = exit_within(&mut serving.child, limit).unwrap_or_else(|| {
        panic!(
            "the server did not exit within {limit:?} of SIGTERM with a connection open: {:?}",
            serving.seen
        )
    });
    assert!(status.success(), "{status:?}: {:?}", serving.seen);
    drop(stuck);
}

/// The same stop, with nothing left to read the server's stderr. The bound on
/// shutdown must not depend on the shutdown lines being writable: a closed
/// pipe is what `serve … 2>&1 | tee log` leaves behind when Ctrl-C ends `tee`
/// too, and the server must still stop within its grace, with success.
#[cfg(unix)]
#[test]
fn serve_exits_on_sigterm_with_a_connection_open_and_stderr_closed() {
    use std::io::Write as _;
    use std::time::Duration;

    let mut serving = serve_listening("sigterm-open-no-stderr", false);
    let mut stuck = std::net::TcpStream::connect(&serving.addr).expect("connects");
    stuck
        .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n")
        .expect("sends half a request head");
    // Long enough for the reader thread to have let go of the pipe, and for
    // the server to have accepted the connection.
    std::thread::sleep(Duration::from_millis(500));

    send_sigterm(&serving.child);

    let limit = taiyaku_serve::SHUTDOWN_GRACE + Duration::from_secs(5);
    let status = exit_within(&mut serving.child, limit).unwrap_or_else(|| {
        panic!("the server did not exit within {limit:?} of SIGTERM with stderr closed")
    });
    assert!(status.success(), "{status:?}");
    drop(stuck);
}

/// A live server keeps its works current once nothing reads its stderr. A
/// refresh with something to say says it there, and with the pipe gone that
/// write fails; the failure must not end the refresher, or `/api/works`, which
/// this test reaches without a lookup miss, stays as it was. The pipe goes the way it does under `serve 2>&1
/// | tee log` once `tee` has.
#[cfg(unix)]
#[test]
fn serve_keeps_its_works_current_with_stderr_closed() {
    use std::time::Duration;
    use taiyaku_library::{Library, account, import};

    let serving = serve_listening_over("live-no-stderr", false, true);
    let library = Library::open(Path::new(&serving.db)).expect("opens the server's library");
    // A key answers only for an account someone has claimed.
    let local = account::promote(&library.conn(), "local", "a password", "admin")
        .expect("claims the seeded account");
    let key = account::create_key(&library.conn(), local, "test").expect("a key for `local`");
    let add = |title: &str, sha: &str| -> i64 {
        let doc = taiyaku_core::Sidecar::from_json(&format!(
            r#"{{"taiyaku":1,"work":{{"title":"{title}","reading":"rtl"}},
                 "editions":{{"raw":{{"label":"raw","members":[
                   {{"path":"/abs/{title} - v01","sha256":"{sha}","pages":4}}]}}}},
                 "hub":"raw","alignments":[]}}"#
        ))
        .expect("parses");
        import::scanned(&library.conn(), &doc, None).expect("imports")
    };
    let broken = add("Broken", "b1");
    let steady = add("Steady", "s1");
    let limit = Duration::from_secs(15);
    assert!(
        listed_within(&serving, &key, "\"Broken\"", limit)
            && listed_within(&serving, &key, "\"Steady\"", limit),
        "the refresher never served the two works"
    );

    // One write the next refresh both serves and complains about: `Broken`
    // will not project, and `Steady` has a new title. The title is the sign
    // that the refresh which wrote to the closed pipe has run.
    library
        .conn()
        .execute_batch(&format!(
            "BEGIN; \
             UPDATE work SET reading = 'sideways' WHERE id = {broken}; \
             UPDATE work SET title = 'Steady, renamed' WHERE id = {steady}; \
             COMMIT;"
        ))
        .expect("writes both");
    assert!(
        listed_within(&serving, &key, "\"Steady, renamed\"", limit),
        "the refresh with a complaint never ran"
    );

    library
        .conn()
        .execute(
            "UPDATE work SET title = 'Steady, again' WHERE id = ?1",
            [steady],
        )
        .expect("renames it again");
    assert!(
        listed_within(&serving, &key, "\"Steady, again\"", limit),
        "the refresher stopped once a complaint could not be written"
    );
}

/// Whether `/api/works`, asked for as `local` with `key`, comes to contain
/// `wanted` within `limit`.
#[cfg(unix)]
fn listed_within(serving: &Serving, key: &str, wanted: &str, limit: std::time::Duration) -> bool {
    use base64::Engine as _;
    use std::io::{Read as _, Write as _};

    let credential = base64::engine::general_purpose::STANDARD.encode(format!("local:{key}"));
    let deadline = std::time::Instant::now() + limit;
    loop {
        let mut stream = std::net::TcpStream::connect(&serving.addr).expect("connects");
        write!(
            stream,
            "GET /api/works HTTP/1.1\r\nHost: {}\r\nAuthorization: Basic {credential}\r\n\
             Connection: close\r\n\r\n",
            serving.addr
        )
        .expect("asks");
        let mut answer = String::new();
        stream
            .read_to_string(&mut answer)
            .expect("reads the answer");
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        if answer.contains(wanted) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

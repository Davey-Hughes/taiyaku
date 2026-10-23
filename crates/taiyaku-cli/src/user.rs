//! `taiyaku library user` and `taiyaku library key`.
//!
//! **Spec A2 §9 puts user administration here rather than in the reader**, and
//! §5 puts bootstrap here as well: promoting the first account is a thing the
//! host's own shell can do, which means an unclaimed server is never reachable
//! over the network waiting to be claimed. The configuration UI is spec C4; the
//! CLI covers it in between, which it has to anyway.

use taiyaku_library::{Library, account};

use crate::library::resolve_db_path;
use crate::{option_value, reject_unknown_flags};

const USER_USAGE: &str = "usage: taiyaku library user promote <name> [--role ROLE] [--db PATH]\n \
                          taiyaku library user add <name> [--role admin|user] [--db PATH]  \
                          (adds a reader; promote makes the administrator)\n \
                          taiyaku library user password <name> [--db PATH]\n \
                          taiyaku library user list [--db PATH]\n \
                          taiyaku library user grant-library|revoke-library <user> <library> \
                          [--db PATH]\n \
                          taiyaku library user every-library <user> [--db PATH]";

const KEY_USAGE: &str = "usage: taiyaku library key create <user> --name DEVICE [--db PATH]\n \
                         taiyaku library key list <user> [--db PATH]\n \
                         taiyaku library key revoke <user> <id> [--db PATH]";

/// Dispatch `library user`.
///
/// # Errors
///
/// Returns an error naming the subcommand if none matches, or whatever the
/// subcommand itself returns.
pub(crate) fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("promote") => promote(&args[1..]),
        Some("add") => add(&args[1..]),
        Some("password") => password(&args[1..]),
        Some("list") => list(&args[1..]),
        Some("grant-library") => grant_library(&args[1..]),
        Some("revoke-library") => revoke_library(&args[1..]),
        Some("every-library") => every_library(&args[1..]),
        Some(verb @ ("grant-root" | "revoke-root")) => Err(renamed(verb)),
        Some(other) => Err(format!(
            "library user has no subcommand `{other}`; it has promote, add, password, list, \
             grant-library, revoke-library and every-library"
        )),
        None => Err(
            "library user needs a subcommand: promote, add, password, list, grant-library, \
                     revoke-library or every-library"
                .to_string(),
        ),
    }
}

/// What `grant-root` and `revoke-root` answer since Part F1: the name of the
/// subcommand that replaced each.
fn renamed(verb: &str) -> String {
    let new = verb.replace("-root", "-library");
    format!(
        "`{verb}` is now `{new}`: a grant names a library. `taiyaku library libraries` lists them"
    )
}

/// Dispatch `library key`.
///
/// # Errors
///
/// Returns an error naming the subcommand if none matches, or whatever the
/// subcommand itself returns.
pub(crate) fn key_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("create") => key_create(&args[1..]),
        Some("list") => key_list(&args[1..]),
        Some("revoke") => key_revoke(&args[1..]),
        Some(other) => Err(format!(
            "library key has no subcommand `{other}`; it has create, list and revoke"
        )),
        None => Err("library key needs a subcommand: create, list or revoke".to_string()),
    }
}

/// The positional arguments before the first flag.
fn positional(args: &[String]) -> Vec<&String> {
    args.iter().take_while(|a| !a.starts_with("--")).collect()
}

fn open(args: &[String]) -> Result<Library, String> {
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    Library::open(&db).map_err(|e| format!("opening the library: {e}"))
}

/// What `promote` gives the account it claims, and what `add` gives a new one.
///
/// **Two constants rather than one default, because the two commands mean
/// different things.** Promoting is how a server gets its first administrator;
/// adding is how a household gets a second reader. One shared default of `admin`
/// served the first correctly and made `taiyaku library user add alice` mint an
/// administrator silently — with `--role` marked optional and the default stated
/// nowhere. Found by the whole-branch review, 2026-08-18.
///
/// **The distinction buys less than it looks like today**, and that is worth
/// knowing rather than discovering: `Principal.role` is resolved by the guard
/// and no route branches on it, so an account of either role can write the
/// shared truth documents and correct alignments. Making the role govern
/// something is spec C4's scope; this is about not handing it out by accident in
/// the meantime.
pub(crate) const PROMOTE_DEFAULT_ROLE: &str = "admin";
pub(crate) const ADD_DEFAULT_ROLE: &str = "user";

/// `admin` or `user`, refused rather than stored as anything else.
///
/// Checked here rather than by a `CHECK` constraint because migration 4 could
/// not add one to a column migration 1 created, and because a typo should be
/// refused with the two words that would have worked rather than by a
/// constraint error naming a column.
fn role_of(args: &[String], default: &str) -> Result<String, String> {
    match option_value(args, "--role", "admin or user")? {
        None => Ok(default.to_string()),
        Some(role @ ("admin" | "user")) => Ok(role.to_string()),
        Some(other) => Err(format!("`{other}` is not a role; it is `admin` or `user`")),
    }
}

/// Read a password twice, without echoing it.
///
/// **Prompted rather than taken as a flag**, so a password never lands in shell
/// history, in `ps`, or in a `--db`-adjacent shared terminal recording. There is
/// no `--password`, deliberately.
///
/// Echo is turned off by shelling out to `stty` rather than through a crate:
/// the workspace forbids `unsafe_code`, which rules out calling `termios`
/// directly, and adding a dependency for it would make this branch's "one new
/// crate" claim two. **If `stty` is not there the read still happens and says
/// so**, because refusing to set a password on a host with an unusual shell
/// would be worse than a visible one the operator can see is visible.
fn read_password(prompt: &str) -> Result<String, String> {
    use std::io::Write as _;

    let quiet = set_echo(false);
    if !quiet {
        eprintln!("warning: could not turn off terminal echo; the password will be visible");
    }
    eprint!("{prompt}: ");
    let _ = std::io::stderr().flush();
    let first = read_line()?;
    if quiet {
        eprintln!();
    }

    eprint!("{prompt} again: ");
    let _ = std::io::stderr().flush();
    let second = read_line()?;
    if quiet {
        eprintln!();
    }
    set_echo(true);
    accept(first, &second)
}

/// The rule about what may be accepted, without the reading.
///
/// Split from [`read_password`] so it can be tested: everything else in that
/// function is a terminal and a blocking read, and the part worth pinning is
/// which inputs are refused and in what words.
fn accept(first: String, second: &str) -> Result<String, String> {
    if first != second {
        return Err("the two passwords did not match".to_string());
    }
    if first.is_empty() {
        // Not pedantry: `credential::LOCKED` is what an account with no
        // password holds, and an empty one hashes to something a login could
        // actually present — so this would create an account that anybody who
        // pressed Enter twice could sign into.
        return Err("an empty password would lock the account rather than open it".to_string());
    }
    Ok(first)
}

fn read_line() -> Result<String, String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("reading the password: {e}"))?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Whether echo could be set. `false` means the password will be visible, which
/// the caller reports rather than swallows.
fn set_echo(on: bool) -> bool {
    std::process::Command::new("stty")
        .arg(if on { "echo" } else { "-echo" })
        .stdin(std::process::Stdio::inherit())
        // Its own complaint is swallowed because this function already reports
        // the outcome in the terms that matter — "the password will be
        // visible" — and `stty: 'standard input': Inappropriate ioctl for
        // device` printed underneath that says the same thing worse.
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// How a command gets the password it needs.
///
/// **Injected so the three prompting commands can be tested at all.** They were
/// unreachable from a test — `read_password` blocks on stdin — so `promote`,
/// `add` and `password` had no coverage below their flag parsing, which is most
/// of what they do. A parameter is the whole of the change: production passes
/// `read_password`, tests pass a closure.
type AskForPassword<'a> = &'a dyn Fn(&str) -> Result<String, String>;

fn promote(args: &[String]) -> Result<(), String> {
    promote_with(args, &read_password)
}

fn add(args: &[String]) -> Result<(), String> {
    add_with(args, &read_password)
}

fn password(args: &[String]) -> Result<(), String> {
    password_with(args, &read_password)
}

fn promote_with(args: &[String], ask: AskForPassword<'_>) -> Result<(), String> {
    reject_unknown_flags("library user promote", args, &["--db", "--role"])?;
    let names = positional(args);
    let [name] = names.as_slice() else {
        return Err(USER_USAGE.to_string());
    };
    let role = role_of(args, PROMOTE_DEFAULT_ROLE)?;
    let library = open(args)?;
    // **Before the prompt, not after.** A library that has already been claimed
    // has nothing to promote, a name the store refuses is never given, and
    // reading a password twice before saying so makes a person do the work and
    // type the secret for a command that was never going to act.
    {
        let conn = library.conn();
        let id = account::unclaimed(&conn)?;
        account::check_name(&conn, name, Some(id)).map_err(|e| e.to_string())?;
    }
    let password = ask("password")?;
    let conn = library.conn();
    let id = account::promote(&conn, name, &password, &role).map_err(|e| e.to_string())?;
    println!("promoted account {id} to `{name}` ({role})");
    println!("everything recorded before now — notes, reading positions — belongs to this account");
    Ok(())
}

fn add_with(args: &[String], ask: AskForPassword<'_>) -> Result<(), String> {
    reject_unknown_flags("library user add", args, &["--db", "--role"])?;
    let names = positional(args);
    let [name] = names.as_slice() else {
        return Err(USER_USAGE.to_string());
    };
    let role = role_of(args, ADD_DEFAULT_ROLE)?;
    let library = open(args)?;
    // Before the prompt, for `promote_with`'s reason: a name the store refuses
    // would be refused only after the password had been typed twice.
    account::check_name(&library.conn(), name, None).map_err(|e| e.to_string())?;
    let password = ask("password")?;
    let conn = library.conn();
    let id = account::add(&conn, name, &password, &role)?;
    println!("added account {id}, `{name}` ({role})");
    Ok(())
}

fn password_with(args: &[String], ask: AskForPassword<'_>) -> Result<(), String> {
    reject_unknown_flags("library user password", args, &["--db"])?;
    let names = positional(args);
    let [name] = names.as_slice() else {
        return Err(USER_USAGE.to_string());
    };
    let library = open(args)?;
    let user_id = {
        let conn = library.conn();
        account::id_of(&conn, name)?.ok_or_else(|| format!("no account named `{name}`"))?
    };
    let password = ask("new password")?;
    let conn = library.conn();
    // `None`: this command holds no session, so there is none to keep. Every
    // session that account has open ends, which is what a password reset from
    // the host's own shell should mean.
    let ended = account::set_password(&conn, user_id, &password, None)?;
    println!("changed the password for `{name}`");
    if let Some(note) = sessions_ended_note(ended) {
        println!("{note}");
    }
    Ok(())
}

/// What `library user password` says about the sessions the change ended, or
/// nothing when it ended none.
///
/// **Extracted from the `if` inside the `println!`**, which is the shape this
/// codebase's mutation sweep keeps finding: a decision that only changes what a
/// person reads, reachable only by running the command. `> 0` survived `==`,
/// `<` and `>=` on 2026-08-19 — three mutants, one of which (`>=`) prints
/// "ended 0 open session(s)" after a password change on an account nobody was
/// signed in as.
fn sessions_ended_note(ended: usize) -> Option<String> {
    (ended > 0).then(|| format!("ended {ended} open session(s); API keys are untouched"))
}

fn list(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user list", args, &["--db"])?;
    let library = open(args)?;
    let conn = library.conn();
    for (account, can_log_in) in account::list(&conn)? {
        let every_library = sees_every_library(&conn, account.id)?;
        let granted: Vec<String> = account::granted_libraries(&conn, account.id)?
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        println!(
            "{}",
            account_line(&account, can_log_in, every_library, &granted)
        );
    }
    Ok(())
}

/// Whether the account `user_id` is set to see every library, as stored: an
/// administrator sees every library whatever this answers.
fn sees_every_library(conn: &rusqlite::Connection, user_id: i64) -> Result<bool, String> {
    account::access_for(conn, user_id)
        .map(|access| access == account::Access::Every)
        .map_err(|e| format!("reading which libraries account {user_id} sees: {e}"))
}

/// One account's line in `user list`: its id, name and role, then what it
/// sees, then a note where it cannot be logged in as.
///
/// **What an account other than an administrator sees is one of three**:
/// every library, the libraries it is granted in the order given, or no
/// library. "No library" is said rather than left blank, so the line is
/// never read as every library. An administrator sees every library whatever
/// is stored, so its line names only the grants it holds for a demotion.
fn account_line(
    account: &account::Account,
    can_log_in: bool,
    every_library: bool,
    granted: &[String],
) -> String {
    let admin = account.role == "admin";
    let grants = if every_library && !admin {
        " — every library".to_string()
    } else if !granted.is_empty() {
        format!(" — granted: {}", granted.join(", "))
    } else if !admin {
        " — no library".to_string()
    } else {
        String::new()
    };
    // "no password set" rather than nothing, because a row that cannot be
    // logged in as looks identical to one that can in every other column,
    // and it is the one a person is looking for when the server will not
    // let them in.
    let state = if can_log_in {
        ""
    } else {
        "  (no password set)"
    };
    format!(
        "{} {} ({}){grants}{state}",
        account.id, account.name, account.role
    )
}

fn key_create(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library key create", args, &["--db", "--name"])?;
    let users = positional(args);
    let [user] = users.as_slice() else {
        return Err(KEY_USAGE.to_string());
    };
    let name = option_value(args, "--name", "a device name")?
        .ok_or("library key create needs --name DEVICE, so the right key can be revoked later")?;
    let library = open(args)?;
    let conn = library.conn();
    let user_id =
        account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
    let secret = account::create_key(&conn, user_id, name)?;
    println!("{secret}");
    eprintln!(
        "This is the only time this key is shown — only its hash is stored. Give it to an \
         OPDS client as the password, with `{user}` as the username."
    );
    Ok(())
}

fn key_list(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library key list", args, &["--db"])?;
    let users = positional(args);
    let [user] = users.as_slice() else {
        return Err(KEY_USAGE.to_string());
    };
    let library = open(args)?;
    let conn = library.conn();
    let user_id =
        account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
    for key in account::keys_of(&conn, user_id)? {
        let used = key.last_used.unwrap_or_else(|| "never used".to_string());
        println!(
            "{} {}  created {}  last used {used}",
            key.id, key.name, key.created
        );
    }
    Ok(())
}

fn key_revoke(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library key revoke", args, &["--db"])?;
    let given = positional(args);
    // **Takes the account as well as the id**, matching `key list <user>` and
    // matching what `account::revoke_key` now scopes on. Before this it took an
    // id alone, so the ownership rule lived at one HTTP call site and nowhere
    // else — and this command could revoke anybody's key. Found by the second
    // whole-branch review.
    let [user, id] = given.as_slice() else {
        return Err(KEY_USAGE.to_string());
    };
    let key_id: i64 = id.parse().map_err(|_| {
        format!("`{id}` is not a key id; `taiyaku library key list <user>` has them")
    })?;
    let library = open(args)?;
    let conn = library.conn();
    let user_id =
        account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
    if account::revoke_key(&conn, user_id, key_id)? {
        println!("revoked API key {key_id} of `{user}`");
    } else {
        println!("`{user}` has no API key {key_id}; nothing to revoke");
    }
    Ok(())
}

const LIBRARY_USAGE: &str = "usage: taiyaku library user grant-library <user> <library> [--db PATH]\n \
                             taiyaku library user revoke-library <user> <library> [--db PATH]\n \
                             taiyaku library user every-library <user> [--db PATH]";

/// `user` and `library`, resolved to the two ids both subcommands need, with
/// the account's name as given and the library's name as the store holds it,
/// which are the names both print.
///
/// **The library is named, not given by a directory's path**, so nothing here
/// is canonicalised and a library is found whether or not its directories can be
/// reached. [`taiyaku_library::libraries::id_of`] looks the name up.
fn grant_target(args: &[String]) -> Result<(Library, String, i64, i64, String), String> {
    let given = positional(args);
    let [user, name] = given.as_slice() else {
        return Err(LIBRARY_USAGE.to_string());
    };
    let library = open(args)?;
    let (user_id, library_id, stored) = {
        let conn = library.conn();
        let user_id =
            account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
        let library_id = taiyaku_library::libraries::id_of(&conn, name)?.ok_or_else(|| {
            format!("no library named `{name}`; `taiyaku library libraries` lists them")
        })?;
        let stored = crate::library::stored_name(&conn, library_id)?;
        (user_id, library_id, stored)
    };
    Ok((library, (*user).clone(), user_id, library_id, stored))
}

fn grant_library(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user grant-library", args, &["--db"])?;
    let (library, user, user_id, library_id, name) = grant_target(args)?;
    let conn = library.conn();
    let was_every = sees_every_library(&conn, user_id)?;
    let was_new = account::grant_library(&conn, user_id, library_id)?;
    if was_new {
        println!("granted sight of {name}");
    } else {
        println!("that account could already see {name}; nothing to do");
    }
    let role = role_of_account(&conn, user_id)?;
    if let Some(note) = admin_note(&role) {
        println!("  {note}");
    }
    if let Some(note) = narrowed_note(&role, was_every, &user) {
        println!("  {note}");
    }
    // **A grant over a library nothing has re-derived shows an empty shelf**,
    // and saying so here is the difference between a puzzling result and a
    // known one. Migration 6 added `member.root_id` and left it NULL;
    // migration 7 backfills what it can from each root's path, but a member
    // whose root cannot be recovered that way stays NULL and `IN (…)` excludes
    // it. Whole-branch review: an operator who granted on such a library got
    // a reader who saw nothing, while this command said the opposite.
    let unrooted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM member WHERE root_id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("counting members with no root: {e}"))?;
    if let Some(note) = unrooted_note(unrooted) {
        println!("  {note}");
    }
    Ok(())
}

/// The role the store holds for the account `user_id`.
fn role_of_account(conn: &rusqlite::Connection, user_id: i64) -> Result<String, String> {
    conn.query_row("SELECT role FROM user WHERE id = ?1", [user_id], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading the account's role: {e}"))
}

/// What a grant says when it narrows an account that saw every library.
///
/// **Said only then, because that is the grant that changes what the account
/// can see the other way.** A grant is "only these", so on an account that
/// saw every library it *narrows* to that one library: an operator who grants
/// a library expecting to widen access has done the opposite, and this is the
/// only moment anything says so. On an account already limited, a grant
/// widens the limit and needs no warning.
///
/// Extracted for the same reason as [`unrooted_note`] and
/// [`sessions_ended_note`]: a unit test reads the decision without running
/// the command.
///
/// **Takes the role since O86.** An administrator is narrowed by nothing —
/// their grant is dropped where the principal is built — so printing this
/// beside [`admin_note`] would tell the operator the grant "does nothing while
/// it stays one" and that the account "is now restricted", one line apart.
fn narrowed_note(role: &str, was_every: bool, user: &str) -> Option<String> {
    (role != "admin" && was_every).then(|| {
        format!(
            "That account saw every library, and is now restricted to that one. \
             `taiyaku library user every-library {user}` lifts the limit."
        )
    })
}

/// The warning a grant carries when its target is an administrator — O86.
///
/// **An admin's grant is recorded and inert.** `auth::Principal::from_account`
/// drops an administrator's roots where the principal is built, because role
/// dominates the other three permission columns: an admin edits shared rows no
/// grant can scope (`series_title` is keyed `(series_id, title)` and has no
/// per-work column to filter on), so a half-sighted one could rename a series
/// for the whole library and leave the works it cannot see holding a stale
/// cached document. That was O86.
///
/// **Said rather than refused.** The row is real and becomes live the moment
/// the account is demoted, so refusing the command would lose a legitimate
/// intent; printing nothing would hand back a result indistinguishable from a
/// working grant.
///
/// Extracted for the same reason as [`narrowed_note`] and [`unrooted_note`]: a
/// unit test reads the decision without running the command.
fn admin_note(role: &str) -> Option<String> {
    (role == "admin").then(|| {
        "That account is an administrator, so this grant does nothing while it \
         stays one — an administrator always sees every library. The grant is \
         recorded, and takes effect if the account is ever demoted."
            .to_string()
    })
}

/// The warning a grant carries when the library holds members with no root.
///
/// **Extracted for the same reason [`sessions_ended_note`] was**, and after
/// the same three mutants: `> 0` inside a `println!` survived as `==`, `<` and
/// `>=`. Each is now a different answer.
///
/// The warning matters because the rule it qualifies is already
/// counter-intuitive: a grant restricts, and a restricted account cannot see a
/// member carrying no root at all — so an operator who grants on a library
/// the migration could not fully backfill gets a reader with less than they
/// expect, and this is the only place that says so.
fn unrooted_note(unrooted: i64) -> Option<String> {
    (unrooted > 0).then(|| {
        format!(
            "Note: {unrooted} member(s) carry no root yet and are invisible to any \
             restricted account. `taiyaku library scan` re-derives them."
        )
    })
}

fn revoke_library(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user revoke-library", args, &["--db"])?;
    let (library, user, user_id, library_id, name) = grant_target(args)?;
    let conn = library.conn();
    let revoked = account::revoke_library(&conn, user_id, library_id)?;
    if revoked {
        println!("revoked sight of {name}");
    } else {
        println!("that account had no grant for {name}; nothing to revoke");
    }
    let role = role_of_account(&conn, user_id)?;
    let held = account::granted_libraries(&conn, user_id)?;
    if let Some(note) = revoked_note(&role, revoked, held.len(), &user) {
        println!("  {note}");
    }
    Ok(())
}

/// What a revocation says about what the account sees now.
///
/// An administrator saw every library before and sees every library after,
/// so where a grant was `revoked` the note says the revocation changes
/// nothing while the account stays one, as [`admin_note`] says of a grant.
/// Any other account whose last grant was `revoked` is told it now sees no
/// library, and how to lift the limit, since revoking never does.
///
/// Extracted for the same reason as [`narrowed_note`]: the role and the count
/// are decided here, where a test can see the answer.
fn revoked_note(role: &str, revoked: bool, held: usize, user: &str) -> Option<String> {
    if role == "admin" {
        revoked.then(|| {
            "That account is an administrator, so this revocation changes nothing while it \
             stays one — an administrator always sees every library."
                .to_string()
        })
    } else {
        (revoked && held == 0).then(|| {
            format!(
                "`{user}` now sees no library; `taiyaku library user every-library {user}` \
                 lifts the limit."
            )
        })
    }
}

/// `every-library`: let an account see every library, including any added
/// later. Revoking an account's last grant leaves it seeing none, so this is
/// the one command that lifts a limit.
fn every_library(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user every-library", args, &["--db"])?;
    let given = positional(args);
    let [user] = given.as_slice() else {
        return Err(LIBRARY_USAGE.to_string());
    };
    let library = open(args)?;
    let conn = library.conn();
    let user_id =
        account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
    let changed = account::set_every_library(&conn, user_id)
        .map_err(|e| format!("letting `{user}` see every library: {e}"))?;
    if changed {
        println!("`{user}` now sees every library");
    } else {
        println!("`{user}` already sees every library; nothing to do");
    }
    Ok(())
}

#[cfg(test)]
mod tests;

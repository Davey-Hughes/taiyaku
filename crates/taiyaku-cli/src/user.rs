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
                          taiyaku library user list [--db PATH]";

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
        Some(other) => Err(format!(
            "library user has no subcommand `{other}`; it has promote, add, password and list"
        )),
        None => Err("library user needs a subcommand: promote, add, password or list".to_string()),
    }
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
    // has nothing to promote, and reading a password twice before saying so
    // makes a person do the work and type the secret for a command that was
    // never going to act.
    {
        let conn = library.conn();
        account::unclaimed(&conn)?;
    }
    let password = ask("password")?;
    let conn = library.conn();
    let id = account::promote(&conn, name, &password, &role)?;
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
    if ended > 0 {
        println!("ended {ended} open session(s); API keys are untouched");
    }
    Ok(())
}

fn list(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user list", args, &["--db"])?;
    let library = open(args)?;
    let conn = library.conn();
    for (account, can_log_in) in account::list(&conn)? {
        // "no password set" rather than nothing, because a row that cannot be
        // logged in as looks identical to one that can in every other column,
        // and it is the one a person is looking for when the server will not
        // let them in.
        let state = if can_log_in {
            String::new()
        } else {
            "  (no password set)".to_string()
        };
        println!("{} {} ({}){state}", account.id, account.name, account.role);
    }
    Ok(())
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

#[cfg(test)]
mod tests;

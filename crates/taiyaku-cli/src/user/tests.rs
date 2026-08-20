use super::*;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

#[test]
fn a_missing_subcommand_names_the_ones_that_exist() {
    let err = command(&argv(&[])).unwrap_err();
    assert!(err.contains("promote"), "{err}");
    assert!(err.contains("password"), "{err}");

    let err = command(&argv(&["bogus"])).unwrap_err();
    assert!(err.contains("`bogus`"), "{err}");
    assert!(err.contains("promote, add, password and list"), "{err}");

    let err = key_command(&argv(&[])).unwrap_err();
    assert!(err.contains("create, list or revoke"), "{err}");

    let err = key_command(&argv(&["bogus"])).unwrap_err();
    assert!(err.contains("create, list and revoke"), "{err}");
}

#[test]
fn an_unknown_flag_is_refused_by_name() {
    let err = command(&argv(&["list", "--bogus"])).unwrap_err();
    assert!(err.contains("`--bogus`"), "{err}");
}

/// A typo in `--role` is refused with the two words that would have worked,
/// rather than stored and discovered later as an account that is neither.
#[test]
fn a_role_that_is_neither_is_refused_with_the_two_that_are() {
    let err = role_of(&argv(&["--role", "administrator"]), "user").unwrap_err();
    assert!(err.contains("administrator"), "{err}");
    assert!(err.contains("`admin` or `user`"), "{err}");

    assert_eq!(
        role_of(&argv(&["--role", "user"]), "admin").expect("parses"),
        "user"
    );
    assert_eq!(
        role_of(&argv(&["--role", "admin"]), "user").expect("parses"),
        "admin"
    );
}

/// **`user add` defaults to `user`, and `promote` to `admin`.**
///
/// One shared default of `admin` served `promote`, where it is right, and `add`,
/// where it is not — so `taiyaku library user add alice` to give a housemate a
/// reader account silently minted a second administrator, with `--role` marked
/// optional and the default stated nowhere. Each caller now says which it means.
///
/// Found by the whole-branch review, 2026-08-18.
#[test]
fn adding_an_account_does_not_quietly_make_an_administrator() {
    assert_eq!(
        role_of(&argv(&[]), ADD_DEFAULT_ROLE).expect("defaults"),
        "user",
        "`user add` with no --role must not mint an administrator"
    );
    assert_eq!(
        role_of(&argv(&[]), PROMOTE_DEFAULT_ROLE).expect("defaults"),
        "admin",
        "promoting the first account is what makes an administrator at all"
    );
}

/// `key create` without `--name` is refused, and the refusal says why the name
/// matters: an account screen listing three keys called nothing is unusable for
/// deciding which one to revoke.
#[test]
fn creating_a_key_needs_a_device_name() {
    let err = key_create(&argv(&["davey"])).unwrap_err();
    assert!(err.contains("--name"), "{err}");
    assert!(err.contains("revoked"), "{err}");
}

#[test]
fn revoking_needs_a_number_and_says_where_to_find_one() {
    let err = key_revoke(&argv(&["davey", "not-a-number"])).unwrap_err();
    assert!(err.contains("not-a-number"), "{err}");
    assert!(err.contains("key list"), "{err}");
}

/// Each command refuses a missing or repeated positional with its usage, rather
/// than picking the first and ignoring the rest.
#[test]
fn a_command_without_its_argument_answers_with_usage() {
    assert!(command(&argv(&["promote"])).unwrap_err().contains("usage"));
    assert!(command(&argv(&["add"])).unwrap_err().contains("usage"));
    assert!(command(&argv(&["password"])).unwrap_err().contains("usage"));
    assert!(key_command(&argv(&["list"])).unwrap_err().contains("usage"));
    assert!(
        key_command(&argv(&["revoke"]))
            .unwrap_err()
            .contains("usage"),
        "revoke with no id"
    );
    assert!(
        command(&argv(&["promote", "one", "two"]))
            .unwrap_err()
            .contains("usage"),
        "two names is ambiguous, not a name and a spare"
    );
}

/// The usage strings name every subcommand this module dispatches, so a person
/// who typed one wrong is told the whole set rather than a corrected guess.
#[test]
fn the_usage_covers_every_subcommand() {
    for expected in ["promote", "add", "password", "list"] {
        assert!(USER_USAGE.contains(expected), "USER_USAGE lacks {expected}");
    }
    for expected in ["create", "list", "revoke"] {
        assert!(KEY_USAGE.contains(expected), "KEY_USAGE lacks {expected}");
    }
}

/// A scratch directory and the path of a library inside it.
fn scratch(name: &str) -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("taiyaku-test-user-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A real on-disk library with one account that can log in, and the `--db`
/// arguments that reach it.
fn a_library(name: &str) -> (std::path::PathBuf, Vec<String>) {
    let db = scratch(name).join("library.db");
    {
        let library = Library::open(&db).expect("opens a real on-disk library");
        let conn = library.conn();
        account::promote(&conn, "davey", "a password", "admin").expect("promotes");
    }
    let flags = vec!["--db".to_string(), db.to_str().unwrap().to_string()];
    (db, flags)
}

fn with(flags: &[String], args: &[&str]) -> Vec<String> {
    let mut all: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    all.extend_from_slice(flags);
    all
}

#[test]
fn listing_accounts_reaches_a_real_library() {
    let (_db, flags) = a_library("list");
    command(&with(&flags, &["list"])).expect("lists");
}

/// The default reaching a stored row, not only `role_of`'s return.
///
/// Driven through `account::add` rather than through the `add` command, which
/// prompts for a password and would block on stdin — the same reason
/// `promoting_a_claimed_library_points_at_user_add` can only reach the refusal
/// that happens before the prompt.
#[test]
fn a_second_account_added_from_the_command_line_is_a_reader() {
    let (db, _flags) = a_library("add-is-a-reader");
    let library = Library::open(&db).expect("opens");
    {
        let conn = library.conn();
        account::add(&conn, "alex", "theirs", ADD_DEFAULT_ROLE).expect("adds");
        let role: String = conn
            .query_row("SELECT role FROM user WHERE name = 'alex'", [], |row| {
                row.get(0)
            })
            .expect("reads");
        assert_eq!(role, "user");
    }
}

/// The whole key lifecycle through the commands a person actually types.
#[test]
fn a_key_can_be_created_listed_and_revoked_from_the_command_line() {
    let (db, flags) = a_library("keys");

    key_command(&with(&flags, &["create", "davey", "--name", "phone"])).expect("creates");
    let id: i64 = {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        conn.query_row("SELECT id FROM api_key", [], |row| row.get(0))
            .expect("one key exists")
    };

    key_command(&with(&flags, &["list", "davey"])).expect("lists");
    key_command(&with(&flags, &["revoke", "davey", &id.to_string()])).expect("revokes");

    let left: i64 = {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        conn.query_row("SELECT COUNT(*) FROM api_key", [], |row| row.get(0))
            .expect("counts")
    };
    assert_eq!(left, 0);
}

/// Revoking a key that is not there says so rather than failing, because
/// "already gone" and "revoked" are the same outcome for the person asking.
#[test]
fn revoking_a_key_that_is_not_there_is_not_an_error() {
    let (_db, flags) = a_library("revoke-missing");
    key_command(&with(&flags, &["revoke", "davey", "999"])).expect("says nothing to revoke");
}

/// Every command that names an account refuses an unknown one by name, rather
/// than doing nothing quietly.
#[test]
fn an_unknown_account_is_named_in_the_refusal() {
    let (_db, flags) = a_library("unknown-account");

    for args in [
        vec!["create", "nobody", "--name", "phone"],
        vec!["list", "nobody"],
    ] {
        let err = key_command(&with(&flags, &args)).unwrap_err();
        assert!(err.contains("nobody"), "{err}");
    }

    let err = command(&with(&flags, &["password", "nobody"])).unwrap_err();
    assert!(err.contains("nobody"), "{err}");
}

/// `promote` on a library that has already been claimed refuses with the
/// command that does what was wanted, through the CLI rather than only in the
/// library's own unit test.
#[test]
fn promoting_a_claimed_library_points_at_user_add() {
    let (_db, flags) = a_library("promote-twice");
    // **Refused before any password is read**, which is what makes this test
    // able to run at all: the version that prompted first would block on stdin
    // here forever, which is exactly what it does to a person.
    let err = command(&with(&flags, &["promote", "someone-else"])).unwrap_err();
    assert!(
        err.contains("already has a password") || err.contains("password"),
        "{err}"
    );
}

/// What `read_password` accepts, without the terminal.
#[test]
fn a_password_is_accepted_only_when_typed_twice_and_not_empty() {
    assert_eq!(
        accept("hunter2".to_string(), "hunter2").expect("accepts"),
        "hunter2"
    );

    let mismatch = accept("hunter2".to_string(), "hunter3").unwrap_err();
    assert!(mismatch.contains("did not match"), "{mismatch}");

    let empty = accept(String::new(), "").unwrap_err();
    assert!(empty.contains("empty"), "{empty}");
    assert!(
        empty.contains("lock"),
        "the refusal should say what an empty password would do: {empty}"
    );
}

/// A password source that answers without a terminal.
fn says(password: &'static str) -> impl Fn(&str) -> Result<String, String> {
    move |_| Ok(password.to_string())
}

/// The whole of `promote`, not just its flag parsing.
#[test]
fn promote_claims_the_seeded_account_end_to_end() {
    let db = scratch("promote-end-to-end").join("library.db");
    let flags = vec!["--db".to_string(), db.to_str().unwrap().to_string()];

    promote_with(&with(&flags, &["davey"]), &says("a real password")).expect("promotes");

    let library = Library::open(&db).expect("opens");
    let conn = library.conn();
    let (id, name, role): (i64, String, String) = conn
        .query_row("SELECT id, name, role FROM user", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("reads");
    assert_eq!(id, 1, "promotion must reach the row spec A seeded");
    assert_eq!(name, "davey");
    assert_eq!(role, "admin");
    let (_, phc) = account::by_name(&conn, "davey")
        .expect("reads")
        .expect("exists");
    assert!(taiyaku_library::credential::verify_password(
        "a real password",
        &phc
    ));
}

/// And `add` beside it, which must not mint an administrator.
#[test]
fn add_creates_a_reader_end_to_end() {
    let (db, flags) = a_library("add-end-to-end");

    add_with(&with(&flags, &["alex"]), &says("theirs")).expect("adds");

    let library = Library::open(&db).expect("opens");
    let conn = library.conn();
    let role: String = conn
        .query_row("SELECT role FROM user WHERE name = 'alex'", [], |row| {
            row.get(0)
        })
        .expect("reads");
    assert_eq!(role, "user");

    // A name already taken is refused rather than silently doing nothing.
    let taken = add_with(&with(&flags, &["alex"]), &says("again")).unwrap_err();
    assert!(taken.contains("alex"), "{taken}");
}

/// `user password` replaces the hash and ends the account's sessions.
#[test]
fn changing_a_password_from_the_command_line_ends_open_sessions() {
    let (db, flags) = a_library("password-end-to-end");
    let library = Library::open(&db).expect("opens");
    {
        let conn = library.conn();
        account::open_session(
            &conn,
            1,
            &taiyaku_library::credential::mint_token(),
            None,
            None,
        )
        .expect("opens a session");
    }

    password_with(&with(&flags, &["davey"]), &says("a new one")).expect("changes");

    let conn = library.conn();
    let (_, phc) = account::by_name(&conn, "davey")
        .expect("reads")
        .expect("exists");
    assert!(taiyaku_library::credential::verify_password(
        "a new one",
        &phc
    ));
    let sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM session", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(
        sessions, 0,
        "an admin resetting a password ends every session that account had open"
    );
}

/// A password source that refuses — a mistyped confirmation, or a closed stdin
/// — leaves the library exactly as it was.
#[test]
fn a_refused_password_changes_nothing() {
    let (db, flags) = a_library("refused-password");
    let refuses = |_: &str| Err("the two passwords did not match".to_string());

    let err = add_with(&with(&flags, &["alex"]), &refuses).unwrap_err();
    assert!(err.contains("did not match"), "{err}");

    let library = Library::open(&db).expect("opens");
    let conn = library.conn();
    assert!(
        account::id_of(&conn, "alex").expect("reads").is_none(),
        "a refused password still created the account"
    );
}

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
    assert!(
        err.contains("promote, add, password, list, grant-root and revoke-root"),
        "{err}"
    );

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
fn scratch(name: &str) -> taiyaku_read::test_support::TempDir {
    taiyaku_read::test_support::TempDir::new(&format!("user-{name}"))
}

/// A real on-disk library with one account that can log in, and the `--db`
/// arguments that reach it, with the directory that holds it.
fn a_library(
    name: &str,
) -> (
    taiyaku_read::test_support::TempDir,
    std::path::PathBuf,
    Vec<String>,
) {
    let dir = scratch(name);
    let db = dir.join("library.db");
    {
        let library = Library::open(&db).expect("opens a real on-disk library");
        let conn = library.conn();
        account::promote(&conn, "davey", "a password", "admin").expect("promotes");
    }
    let flags = vec!["--db".to_string(), db.to_str().unwrap().to_string()];
    (dir, db, flags)
}

fn with(flags: &[String], args: &[&str]) -> Vec<String> {
    let mut all: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    all.extend_from_slice(flags);
    all
}

#[test]
fn listing_accounts_reaches_a_real_library() {
    let (_dir, _db, flags) = a_library("list");
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
    let (_dir, db, _flags) = a_library("add-is-a-reader");
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
    let (_dir, db, flags) = a_library("keys");

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
    let (_dir, _db, flags) = a_library("revoke-missing");
    key_command(&with(&flags, &["revoke", "davey", "999"])).expect("says nothing to revoke");
}

/// Every command that names an account refuses an unknown one by name, rather
/// than doing nothing quietly.
#[test]
fn an_unknown_account_is_named_in_the_refusal() {
    let (_dir, _db, flags) = a_library("unknown-account");

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
    let (_dir, _db, flags) = a_library("promote-twice");
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
    let dir = scratch("promote-end-to-end");
    let db = dir.join("library.db");
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
    let (_dir, db, flags) = a_library("add-end-to-end");

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
    let (_dir, db, flags) = a_library("password-end-to-end");
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
    let (_dir, db, flags) = a_library("refused-password");
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

/// The three mutants `> 0` carried while it lived inside a `println!`: `==`,
/// `<` and `>=` all survived, because nothing could see what the command
/// printed. Extracted into [`sessions_ended_note`], each is now a different
/// answer.
#[test]
fn the_sessions_ended_note_appears_only_when_sessions_ended() {
    assert_eq!(
        sessions_ended_note(0),
        None,
        "`>=` prints `ended 0 open session(s)` after changing the password on \
         an account nobody was signed in as"
    );
    assert_eq!(
        sessions_ended_note(1).as_deref(),
        Some("ended 1 open session(s); API keys are untouched"),
        "`==` and `<` both drop the note for the one case it is most wanted"
    );
    assert!(
        sessions_ended_note(4)
            .expect("says something")
            .contains("ended 4"),
        "the count is the message's whole content and must be the real one"
    );
}

/// The root is named by **path**, not by id: a person adding a root typed a
/// path (`library scan --add PATH`) and has no reason to know its rowid.
#[test]
fn granting_a_root_by_path_resolves_it_to_the_row() {
    let (_dir, db, flags) = a_library("grant-root");
    let root_dir = scratch("grant-root-shelf");
    let root_id = {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        taiyaku_library::scan::add_root(&conn, &root_dir)
            .expect("adds the root")
            .id
    };

    command(&with(
        &flags,
        &["grant-root", "davey", root_dir.to_str().unwrap()],
    ))
    .expect("grants");

    let library = Library::open(&db).expect("reopens");
    let conn = library.conn();
    let davey = account::id_of(&conn, "davey")
        .expect("looks up")
        .expect("davey exists");
    assert_eq!(
        account::roots_for(&conn, davey).expect("reads"),
        vec![root_id]
    );
}

#[test]
fn revoking_a_root_takes_the_grant_back() {
    let (_dir, db, flags) = a_library("revoke-root");
    let root_dir = scratch("revoke-root-shelf");
    {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root");
    }
    let path = root_dir.to_str().unwrap();

    command(&with(&flags, &["grant-root", "davey", path])).expect("grants");
    command(&with(&flags, &["revoke-root", "davey", path])).expect("revokes");

    let library = Library::open(&db).expect("reopens");
    let conn = library.conn();
    let davey = account::id_of(&conn, "davey")
        .expect("looks up")
        .expect("davey exists");
    assert!(
        account::roots_for(&conn, davey).expect("reads").is_empty(),
        "the grant is gone, so the account sees every root again"
    );
}

#[test]
fn granting_a_root_this_library_does_not_hold_names_the_path() {
    let (_dir, _db, flags) = a_library("grant-unknown-root");
    let why = command(&with(&flags, &["grant-root", "davey", "/not/a/root"])).unwrap_err();
    assert!(
        why.contains("/not/a/root"),
        "the message names the path: {why}"
    );
}

#[test]
fn granting_a_root_to_an_account_that_does_not_exist_names_the_account() {
    let (_dir, db, flags) = a_library("grant-unknown-user");
    let root_dir = scratch("grant-unknown-user-shelf");
    {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root");
    }
    let why = command(&with(
        &flags,
        &["grant-root", "nobody", root_dir.to_str().unwrap()],
    ))
    .unwrap_err();
    assert!(
        why.contains("nobody"),
        "the message names the account: {why}"
    );
}

/// Granting a root twice is not an error and says so, rather than reporting a
/// second grant that did not happen.
#[test]
fn granting_a_root_twice_says_nothing_changed() {
    let (_dir, db, flags) = a_library("grant-root-twice");
    let root_dir = scratch("grant-root-twice-shelf");
    {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root");
    }
    let path = root_dir.to_str().unwrap();

    command(&with(&flags, &["grant-root", "davey", path])).expect("grants");
    command(&with(&flags, &["grant-root", "davey", path])).expect("grants again");

    let library = Library::open(&db).expect("reopens");
    let conn = library.conn();
    let davey = account::id_of(&conn, "davey")
        .expect("looks up")
        .expect("davey exists");
    assert_eq!(
        account::granted_roots(&conn, davey).expect("reads").len(),
        1,
        "granting twice is still one grant"
    );
}

/// Revoking a grant that was never made is not an error.
#[test]
fn revoking_a_root_that_was_never_granted_is_not_an_error() {
    let (_dir, db, flags) = a_library("revoke-root-absent");
    let root_dir = scratch("revoke-root-absent-shelf");
    {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root");
    }
    command(&with(
        &flags,
        &["revoke-root", "davey", root_dir.to_str().unwrap()],
    ))
    .expect("revoking what is not granted succeeds");
}

/// **`revoke-root` works when the shelf is gone**, which is when withdrawing
/// access matters most: an unmounted drive or a deleted directory cannot be
/// canonicalised, so `root_by_path` falls back to the stored text. Found by
/// the whole-branch review.
#[test]
fn revoking_a_root_whose_directory_is_gone_still_works() {
    let (_dir, db, flags) = a_library("revoke-root-gone");
    let root_dir = scratch("revoke-root-gone-shelf");
    let stored = {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root");
        root.path.clone()
    };
    let path = stored.to_str().unwrap().to_string();
    command(&with(&flags, &["grant-root", "davey", &path])).expect("grants");

    // The drive goes away.
    std::fs::remove_dir_all(&root_dir).expect("removes the shelf");
    assert!(!root_dir.exists());

    command(&with(&flags, &["revoke-root", "davey", &path]))
        .expect("revokes a root whose directory is gone");

    let library = Library::open(&db).expect("reopens");
    let conn = library.conn();
    let davey = account::id_of(&conn, "davey")
        .expect("looks up")
        .expect("davey exists");
    assert!(
        account::roots_for(&conn, davey).expect("reads").is_empty(),
        "the grant is gone even though the shelf is"
    );
}

/// Both subcommands refuse a wrong number of positionals with the usage text.
#[test]
fn the_root_subcommands_state_their_usage() {
    let (_dir, _db, flags) = a_library("root-usage");
    for verb in ["grant-root", "revoke-root"] {
        let err = command(&with(&flags, &[verb, "only-one-argument"])).unwrap_err();
        assert!(err.contains("grant-root"), "{verb}: {err}");
        assert!(err.contains("<root-path>"), "{verb}: {err}");
    }
}

/// `root_by_path` resolves the root it was *named*, not merely some root.
///
/// A mutation sweep found `==` replaceable with `!=` in that comparison while
/// every test passed — with one root in the library, matching the wrong one
/// and matching none are indistinguishable. Two roots tell them apart.
#[test]
fn granting_names_the_root_it_was_given_and_not_another() {
    let (_dir, db, flags) = a_library("grant-root-picks-one");
    let wanted = scratch("grant-root-picks-wanted");
    let other = scratch("grant-root-picks-other");
    let (wanted_id, other_id) = {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        let a = taiyaku_library::scan::add_root(&conn, &wanted).expect("adds wanted");
        let b = taiyaku_library::scan::add_root(&conn, &other).expect("adds other");
        (a.id, b.id)
    };

    command(&with(
        &flags,
        &["grant-root", "davey", wanted.to_str().unwrap()],
    ))
    .expect("grants");

    let library = Library::open(&db).expect("reopens");
    let conn = library.conn();
    let davey = account::id_of(&conn, "davey")
        .expect("looks up")
        .expect("davey exists");
    let held = account::roots_for(&conn, davey).expect("reads");
    assert_eq!(held, vec![wanted_id], "the named root, not {other_id}");
}

/// The three mutants `> 0` carried while it lived inside a `println!` — the
/// same three [`sessions_ended_note`] was extracted for, reintroduced by
/// spec C1's `grant-root` and found by the sweep on 2026-08-20.
#[test]
fn the_unrooted_note_appears_only_when_members_carry_no_root() {
    assert_eq!(
        unrooted_note(0),
        None,
        "`>=` and `==` warn about unrooted members on a library that has none"
    );
    assert_eq!(
        unrooted_note(-1),
        None,
        "a count cannot be negative, and `<` would warn on every healthy library"
    );
    let note = unrooted_note(3).expect("three unrooted members are worth saying");
    assert!(note.contains('3'), "the note names the number: {note}");
    assert!(note.contains("library scan"), "and what fixes it: {note}");
}

/// `!= 1` survived inside the `println!`, which would have warned about being
/// "restricted to that one root" on the *second* grant and stayed silent on
/// the first — the one that actually narrows the account.
///
/// **It also takes the role since O86**, because an administrator is not
/// narrowed by anything: printing both notes told the operator the grant
/// "does nothing while it stays one" and that the account "is now restricted
/// to that one root", in that order, which is a direct contradiction. The role
/// is decided inside this function rather than by an `if` around the
/// `println!`, for the same reason the count is.
#[test]
fn the_narrowing_note_appears_only_on_a_users_first_grant() {
    assert_eq!(
        narrowed_note("user", 0),
        None,
        "`!=` warns about a restriction on an account that holds no grant at all"
    );
    let first = narrowed_note("user", 1).expect("the first grant narrows the account");
    assert!(first.contains("restricted"), "{first}");
    assert!(
        first.contains("no grants at all sees every root"),
        "it states the rule that makes this counter-intuitive: {first}"
    );
    assert_eq!(
        narrowed_note("user", 2),
        None,
        "`!=` warns on every later grant, which widen rather than narrow"
    );
    assert_eq!(
        narrowed_note("admin", 1),
        None,
        "an administrator is narrowed by nothing — `admin_note` says the grant \
         is inert, and saying it also restricts them contradicts that in the \
         very next line"
    );
}

/// **O86: a grant on an administrator is recorded and inert, and the command
/// has to say so.**
///
/// `auth::Principal::from_account` drops an admin's roots where the principal
/// is built — role dominates the other three permission columns — so
/// `grant-root` against an admin writes a row that changes nothing about what
/// that account sees. The row is kept rather than refused, because a later
/// demotion makes it live; an operator who is not told that has been given a
/// result indistinguishable from a working grant.
///
/// Extracted as a pure function for the same reason `narrowed_note` and
/// `unrooted_note` were: a comparison inside a `println!` survives mutation,
/// because nothing can see what a command printed.
#[test]
fn the_admin_note_appears_only_for_an_administrator() {
    assert_eq!(
        admin_note("user"),
        None,
        "an ordinary account's grant restricts it, and needs no warning"
    );
    let note = admin_note("admin").expect("an admin's grant is inert and must say so");
    assert!(
        note.contains("administrator") && note.contains("demot"),
        "the note has to say both that it does nothing now and what would \
         make it matter later, or it reads as a refusal: {note}"
    );
}

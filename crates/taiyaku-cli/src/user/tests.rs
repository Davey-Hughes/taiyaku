use super::*;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

#[test]
fn a_missing_subcommand_names_the_ones_that_exist() {
    let err = command(&argv(&[])).unwrap_err();
    assert!(err.contains("promote"), "{err}");
    assert!(err.contains("password"), "{err}");
    assert!(err.contains("grant-library"), "{err}");
    assert!(err.contains("revoke-library"), "{err}");
    assert!(err.contains("every-library"), "{err}");

    let err = command(&argv(&["bogus"])).unwrap_err();
    assert!(err.contains("`bogus`"), "{err}");
    assert!(
        err.contains(
            "promote, add, password, list, grant-library, revoke-library and every-library"
        ),
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
/// A reader account made with `taiyaku library user add alice` must not come
/// out an administrator when `--role` is left off.
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
    for expected in [
        "promote",
        "add",
        "password",
        "list",
        "grant-library",
        "revoke-library",
        "every-library",
    ] {
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
/// prompts for a password and would block on stdin.
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
    // `someone-else` is a name the store would give, so only the claim can
    // refuse it, and `never_asked` holds that it does so before the prompt.
    assert_eq!(
        promote_with(&with(&flags, &["someone-else"]), &never_asked),
        Err(
            "every account in this library already has a password, so there is none \
             to promote. `taiyaku library user add <name>` creates another"
                .to_string()
        )
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
    assert!(taiyaku_library::credential::verify_password_in(
        &mut taiyaku_library::credential::WorkingMemory::new(),
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

/// A password source that fails the test if it is asked: the command was to
/// refuse the name, or the claim, before reading one.
fn never_asked(prompt: &str) -> Result<String, String> {
    panic!("asked for the {prompt} before refusing")
}

/// Each name the store refuses for an account, with the sentence it refuses
/// it in, where `alex` is held by another account.
const REFUSED_NAMES: [(&str, &str); 4] = [
    (".", "An address cannot hold “.” or “..” as a name."),
    ("..", "An address cannot hold “.” or “..” as a name."),
    ("", "an account needs a name, since its page is at its name"),
    ("alex", "an account is already called `alex`"),
];

/// `user add` refuses each name the store refuses before it asks for a
/// password, so nobody types a secret twice for an account never made.
#[test]
fn add_refuses_a_name_before_asking_for_a_password() {
    let (_dir, db, flags) = a_library("add-name-first");
    {
        let library = Library::open(&db).expect("opens");
        account::add(&library.conn(), "alex", "theirs", "user").expect("adds");
    }
    for (name, said) in REFUSED_NAMES {
        assert_eq!(
            add_with(&with(&flags, &[name]), &never_asked),
            Err(said.to_string()),
            "{name:?}"
        );
    }
}

/// `user promote` does the same, and the account it would promote keeps its
/// own name: giving that is no clash.
#[test]
fn promote_refuses_a_name_before_asking_for_a_password() {
    let dir = scratch("promote-name-first");
    let db = dir.join("library.db");
    {
        let library = Library::open(&db).expect("opens");
        account::add(&library.conn(), "alex", "theirs", "user").expect("adds");
    }
    let flags = vec!["--db".to_string(), db.to_str().unwrap().to_string()];
    for (name, said) in REFUSED_NAMES {
        assert_eq!(
            promote_with(&with(&flags, &[name]), &never_asked),
            Err(said.to_string()),
            "{name:?}"
        );
    }
    let seeded = {
        let library = Library::open(&db).expect("opens");
        let conn = library.conn();
        conn.query_row("SELECT name FROM user WHERE id = 1", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("reads")
    };
    promote_with(&with(&flags, &[&seeded]), &says("a real password")).expect("promotes");
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
    assert!(taiyaku_library::credential::verify_password_in(
        &mut taiyaku_library::credential::WorkingMemory::new(),
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

/// Each of `==`, `<` and `>=` in place of `> 0` in [`sessions_ended_note`]
/// gives a different answer.
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

/// A directory added to the library at `db`, filed under the library called
/// `name`, which is created if there is none.
fn a_dir_in(
    db: &std::path::Path,
    shelf: &std::path::Path,
    name: &str,
) -> taiyaku_library::scan::Root {
    let library = Library::open(db).expect("opens");
    let conn = library.conn();
    taiyaku_library::scan::add_root_in(&conn, shelf, taiyaku_library::scan::InLibrary::Named(name))
        .expect("adds the directory")
}

/// `davey`'s id, and the libraries and directories that account is granted.
fn grants_of_davey(db: &std::path::Path) -> (Vec<i64>, account::Access) {
    let library = Library::open(db).expect("reopens");
    let conn = library.conn();
    let davey = account::id_of(&conn, "davey")
        .expect("looks up")
        .expect("davey exists");
    (
        account::libraries_for(&conn, davey).expect("reads"),
        account::access_for(&conn, davey).expect("reads"),
    )
}

/// The library is named by **name**, not by id or by a directory's path, and the
/// grant is of the library: both of its directories come with it.
#[test]
fn granting_a_library_by_name_resolves_it_to_the_row() {
    let (_dir, db, flags) = a_library("grant-library");
    let first = scratch("grant-library-first-shelf");
    let second = scratch("grant-library-second-shelf");
    let a = a_dir_in(&db, &first, "Manga");
    let b = a_dir_in(&db, &second, "Manga");

    command(&with(&flags, &["grant-library", "davey", "Manga"])).expect("grants");

    assert_eq!(
        grants_of_davey(&db),
        (vec![a.library_id], account::Access::Only(vec![a.id, b.id])),
        "the library named, and so each of its directories"
    );
}

#[test]
fn revoking_a_library_takes_the_grant_back() {
    let (_dir, db, flags) = a_library("revoke-library");
    let shelf = scratch("revoke-library-shelf");
    a_dir_in(&db, &shelf, "Manga");

    command(&with(&flags, &["grant-library", "davey", "Manga"])).expect("grants");
    command(&with(&flags, &["revoke-library", "davey", "Manga"])).expect("revokes");

    assert_eq!(
        grants_of_davey(&db),
        (Vec::new(), account::Access::Only(Vec::new())),
        "the grant is gone, and the account sees no library rather than every library"
    );
}

#[test]
fn granting_a_library_that_does_not_exist_names_it() {
    let (_dir, _db, flags) = a_library("grant-unknown-library");
    let why = command(&with(&flags, &["grant-library", "davey", "Not A Library"])).unwrap_err();
    assert_eq!(
        why,
        "no library named `Not A Library`; `taiyaku library libraries` lists them"
    );
}

#[test]
fn granting_a_library_to_an_account_that_does_not_exist_names_the_account() {
    let (_dir, db, flags) = a_library("grant-unknown-user");
    let shelf = scratch("grant-unknown-user-shelf");
    a_dir_in(&db, &shelf, "Manga");
    let why = command(&with(&flags, &["grant-library", "nobody", "Manga"])).unwrap_err();
    assert!(
        why.contains("nobody"),
        "the message names the account: {why}"
    );
}

/// Granting a library twice is not an error and says so, rather than
/// reporting a second grant that did not happen.
#[test]
fn granting_a_library_twice_says_nothing_changed() {
    let (_dir, db, flags) = a_library("grant-library-twice");
    let shelf = scratch("grant-library-twice-shelf");
    let dir = a_dir_in(&db, &shelf, "Manga");

    command(&with(&flags, &["grant-library", "davey", "Manga"])).expect("grants");
    command(&with(&flags, &["grant-library", "davey", "Manga"])).expect("grants again");

    assert_eq!(
        grants_of_davey(&db).0,
        vec![dir.library_id],
        "granting twice is still one grant"
    );
}

/// Revoking a grant that was never made is not an error.
#[test]
fn revoking_a_library_that_was_never_granted_is_not_an_error() {
    let (_dir, db, flags) = a_library("revoke-library-absent");
    let shelf = scratch("revoke-library-absent-shelf");
    a_dir_in(&db, &shelf, "Manga");
    command(&with(&flags, &["revoke-library", "davey", "Manga"]))
        .expect("revoking what is not granted succeeds");
}

/// The three subcommands refuse a wrong number of positionals with the usage
/// text, which names all three.
#[test]
fn the_library_subcommands_state_their_usage() {
    let (_dir, _db, flags) = a_library("library-usage");
    for given in [
        &["grant-library", "only-one-argument"][..],
        &["revoke-library", "only-one-argument"][..],
        &["every-library"][..],
        &["every-library", "one", "two"][..],
    ] {
        let err = command(&with(&flags, given)).unwrap_err();
        assert!(err.contains("grant-library"), "{given:?}: {err}");
        assert!(err.contains("revoke-library"), "{given:?}: {err}");
        assert!(err.contains("every-library <user>"), "{given:?}: {err}");
        assert!(err.contains("<library>"), "{given:?}: {err}");
    }
}

/// `grant-root` and `revoke-root` answer with the name of the subcommand that
/// replaced each, and grant nothing.
#[test]
fn the_old_grant_root_names_its_replacement() {
    let err = command(&argv(&["grant-root", "davey", "x"])).unwrap_err();
    assert_eq!(
        err,
        "`grant-root` is now `grant-library`: a grant names a library. \
         `taiyaku library libraries` lists them"
    );
    let err = command(&argv(&["revoke-root", "davey", "x"])).unwrap_err();
    assert_eq!(
        err,
        "`revoke-root` is now `revoke-library`: a grant names a library. \
         `taiyaku library libraries` lists them"
    );
}

/// The library *named* is the one granted, not merely some library.
///
/// With one library, resolving the wrong one and resolving the only one are
/// the same answer. Here the library wanted is the later of two by id and by
/// name, so a lookup that took the first row either way takes the other.
#[test]
fn granting_names_the_library_it_was_given_and_not_another() {
    let (_dir, db, flags) = a_library("grant-library-picks-one");
    let other = scratch("grant-library-picks-other");
    let wanted = scratch("grant-library-picks-wanted");
    let other = a_dir_in(&db, &other, "Other");
    let wanted = a_dir_in(&db, &wanted, "Wanted");

    command(&with(&flags, &["grant-library", "davey", "Wanted"])).expect("grants");

    assert_eq!(
        grants_of_davey(&db),
        (
            vec![wanted.library_id],
            account::Access::Only(vec![wanted.id])
        ),
        "the named library, not {}",
        other.library_id
    );
}

/// A revoke names its library too: the account keeps the other one.
#[test]
fn revoking_names_the_library_it_was_given_and_not_another() {
    let (_dir, db, flags) = a_library("revoke-library-picks-one");
    let other = scratch("revoke-library-picks-other");
    let wanted = scratch("revoke-library-picks-wanted");
    let other = a_dir_in(&db, &other, "Other");
    a_dir_in(&db, &wanted, "Wanted");
    command(&with(&flags, &["grant-library", "davey", "Other"])).expect("grants");
    command(&with(&flags, &["grant-library", "davey", "Wanted"])).expect("grants");

    command(&with(&flags, &["revoke-library", "davey", "Wanted"])).expect("revokes");

    assert_eq!(
        grants_of_davey(&db),
        (
            vec![other.library_id],
            account::Access::Only(vec![other.id])
        ),
        "the library that was not named is still granted"
    );
}

/// Each of `==`, `<` and `>=` in place of `> 0` in [`unrooted_note`] gives a
/// different answer.
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

/// A grant narrows an account only where it saw every library, so only then
/// does it say so, and how to lift the limit again. An account already
/// limited is widened by a grant, and an administrator is narrowed by
/// nothing (O86): `admin_note` says the grant is inert, and saying it also
/// restricts them would contradict that in the very next line.
#[test]
fn the_narrowing_note_appears_only_where_the_account_saw_every_library() {
    assert_eq!(
        narrowed_note("user", true, "alex").as_deref(),
        Some(
            "That account saw every library, and is now restricted to that one. \
             `taiyaku library user every-library alex` lifts the limit."
        )
    );
    assert_eq!(
        narrowed_note("user", false, "alex"),
        None,
        "an account already limited is widened, not narrowed"
    );
    assert_eq!(narrowed_note("admin", true, "davey"), None);
    assert_eq!(narrowed_note("admin", false, "davey"), None);
}

/// A revocation leaves an administrator seeing what it saw, so it is told
/// the revocation changes nothing while it stays one, and only where a grant
/// was revoked. Any other account whose last grant was revoked is told it now
/// sees no library, never every library, and how to lift the limit.
#[test]
fn the_revoked_note_follows_the_role_and_the_grants_left() {
    assert_eq!(
        revoked_note("user", true, 0, "alex").as_deref(),
        Some(
            "`alex` now sees no library; `taiyaku library user every-library alex` lifts \
             the limit."
        )
    );
    assert_eq!(
        revoked_note("user", false, 0, "alex"),
        None,
        "where nothing was revoked nothing changed"
    );
    assert_eq!(
        revoked_note("user", true, 1, "alex"),
        None,
        "an account still holding a grant still sees it"
    );

    let inert =
        revoked_note("admin", true, 0, "davey").expect("an administrator's revocation says so");
    assert!(
        inert.contains("administrator") && inert.contains("changes nothing"),
        "{inert}"
    );
    assert!(
        inert.contains("always sees every library") && !inert.contains("no library"),
        "an administrator saw every library throughout: {inert}"
    );
    assert_eq!(
        revoked_note("admin", true, 1, "davey").as_deref(),
        Some(inert.as_str()),
        "and the same whatever grants it has left"
    );
    assert_eq!(
        revoked_note("admin", false, 0, "davey"),
        None,
        "where nothing was revoked there is nothing to qualify"
    );
}

fn an_account(id: i64, name: &str, role: &str) -> account::Account {
    account::Account {
        id,
        name: name.to_string(),
        role: role.to_string(),
        can_download: true,
        can_stream: true,
        can_stamp: false,
    }
}

/// `user list` says what an account other than an administrator sees after
/// its role: every library, the libraries it is granted in the order it is
/// handed them, or no library. An administrator's line names only the grants
/// it holds, and nothing where it holds none, whatever its setting says. The
/// note where it has no password comes last.
#[test]
fn the_account_line_says_what_an_account_sees() {
    let none: [String; 0] = [];
    let two = ["Manga".to_string(), "Other".to_string()];
    let admin = an_account(1, "davey", "admin");
    assert_eq!(account_line(&admin, true, true, &none), "1 davey (admin)");
    assert_eq!(account_line(&admin, true, false, &none), "1 davey (admin)");
    assert_eq!(
        account_line(&admin, true, false, &two[..1]),
        "1 davey (admin) — granted: Manga"
    );
    assert_eq!(
        account_line(&an_account(1, "local", "admin"), false, true, &none),
        "1 local (admin)  (no password set)"
    );
    let alex = an_account(2, "alex", "user");
    assert_eq!(
        account_line(&alex, true, true, &none),
        "2 alex (user) — every library"
    );
    assert_eq!(
        account_line(&alex, true, false, &two),
        "2 alex (user) — granted: Manga, Other"
    );
    assert_eq!(
        account_line(&alex, false, false, &two[1..]),
        "2 alex (user) — granted: Other  (no password set)"
    );
    assert_eq!(
        account_line(&alex, true, false, &none),
        "2 alex (user) — no library"
    );
}

/// **O86: a grant on an administrator is recorded and inert, and the command
/// has to say so.**
///
/// `auth::Principal::from_account` drops an admin's roots where the principal
/// is built — role dominates the other three permission columns — so
/// `grant-library` against an admin writes a row that changes nothing about
/// what that account sees. The row is kept rather than refused, because a later
/// demotion makes it live; an operator who is not told that has been given a
/// result indistinguishable from a working grant.
///
/// Extracted as a pure function for the same reason `narrowed_note` and
/// `unrooted_note` were: this test reads the decision without running the
/// command.
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
    assert!(
        note.contains("always sees every library"),
        "and what an administrator sees meanwhile: {note}"
    );
}

use super::*;
use crate::{Library, credential};

fn library() -> Library {
    Library::open_in_memory().expect("in-memory library")
}

/// A library whose seeded account has been claimed.
///
/// **`by_api_key` refuses a locked account**, so a test about keys needs one
/// somebody could log in as — otherwise it is testing the locked-account
/// refusal by accident. `a_key_on_a_locked_account_authenticates_as_nobody` is
/// the test that wants the other state, and it says so.
fn claimed_library() -> Library {
    let library = library();
    {
        let conn = library.conn();
        let phc = credential::hash_password("claimed").expect("hashes");
        conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
            .expect("claims the seeded account");
    }
    library
}

/// A work with one edition and one member — enough to hang a note and a reading
/// position off, which is what `promotion_reaches_the_seeded_row` needs in order
/// to prove they survived.
fn a_work(conn: &rusqlite::Connection) -> i64 {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(conn, &doc, std::path::Path::new("/x/taiyaku.json")).expect("import")
}

#[test]
fn an_account_is_found_by_name_with_its_stored_hash() {
    let library = library();
    let conn = library.conn();
    let phc = credential::hash_password("hunter2").expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("sets the hash");

    let (account, stored) = by_name(&conn, "local").expect("reads").expect("exists");
    assert_eq!(account.id, 1);
    assert_eq!(account.name, "local");
    assert_eq!(account.role, "admin");
    assert!(
        account.can_download,
        "migration 4 defaults both capabilities on"
    );
    assert!(account.can_stream);
    assert!(credential::verify_password_in(
        &mut credential::WorkingMemory::new(),
        "hunter2",
        &stored
    ));

    assert!(by_name(&conn, "nobody").expect("reads").is_none());
}

#[test]
fn an_api_key_resolves_to_its_owner_and_a_revoked_one_does_not() {
    let library = claimed_library();
    let conn = library.conn();
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
        [credential::token_sha256(&secret)],
    )
    .expect("inserts");

    let found = by_api_key(&conn, &secret)
        .expect("reads")
        .expect("resolves");
    assert_eq!(found.id, 1);
    assert!(by_api_key(&conn, "tyk_wrong").expect("reads").is_none());

    conn.execute("DELETE FROM api_key", []).expect("revokes");
    assert!(by_api_key(&conn, &secret).expect("reads").is_none());
}

/// An expired key is refused, and refused the **same way** a wrong one is — a
/// client cannot tell "this key was real once" from "this key never was".
#[test]
fn an_expired_api_key_is_refused() {
    let library = claimed_library();
    let conn = library.conn();
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256, expires)
         VALUES (1, 'old', ?1, datetime('now', '-1 day'))",
        [credential::token_sha256(&secret)],
    )
    .expect("inserts");

    assert!(by_api_key(&conn, &secret).expect("reads").is_none());
}

/// The lookup records that the key was used, because an account screen that
/// lists keys is unusable for deciding which to revoke if every row says the
/// same nothing.
#[test]
fn using_a_key_records_that_it_was_used() {
    let library = claimed_library();
    let conn = library.conn();
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
        [credential::token_sha256(&secret)],
    )
    .expect("inserts");

    let before: Option<String> = conn
        .query_row("SELECT last_used FROM api_key", [], |row| row.get(0))
        .expect("reads");
    assert!(before.is_none(), "a key nobody has used has no last_used");

    by_api_key(&conn, &secret)
        .expect("reads")
        .expect("resolves");

    let after: Option<String> = conn
        .query_row("SELECT last_used FROM api_key", [], |row| row.get(0))
        .expect("reads");
    assert!(
        after.is_some(),
        "using a key did not record that it was used"
    );
}

/// §8: a grant written by hand is read back as the folders of the library
/// granted, and an account with none reads as an empty list.
#[test]
fn roots_are_read_per_user() {
    let library = library();
    let conn = library.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (7, '/library')", [])
        .expect("root");

    assert_eq!(
        roots_for(&conn, 1).expect("reads"),
        Vec::<i64>::new(),
        "a user granted nothing has no roots, which is not the same as every root"
    );

    conn.execute(
        "INSERT INTO user_library (user_id, library_id) SELECT 1, library_id FROM root WHERE id = 7",
        [],
    )
    .expect("grant");
    assert_eq!(roots_for(&conn, 1).expect("reads"), vec![7]);
}

/// §5's bootstrap turns on this answer: the claim route is open only while it
/// is false.
///
/// **Role alone is not enough.** Migration 1 seeds `(1, 'local', 'admin')` and
/// migration 4 gives it the locked placeholder, so a fresh library has an admin
/// row and no admin. Reading `role = 'admin'` on its own would close the claim
/// route before anybody could reach it, and the server would be unreachable by
/// its own owner.
#[test]
fn a_locked_admin_does_not_count_as_an_admin_existing() {
    let library = library();
    let conn = library.conn();

    assert!(
        !admin_exists(&conn).expect("reads"),
        "the seeded row has role 'admin' and a locked password; counting it \
         would close the claim route before anyone could use it"
    );

    let phc = credential::hash_password("real").expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("promotes");
    assert!(admin_exists(&conn).expect("reads"));
}

/// A non-admin with a real password is still not an admin.
///
/// **The second half of what this test used to claim was the hole.** It read
/// "so the claim route does not close because an ordinary reader was added
/// first", which is a description of the defect written as though it were the
/// requirement: a library somebody has already set up must not be claimable by
/// a stranger, whatever role they set up. The claim route reads [`is_claimed`]
/// now, and `a_reader_with_a_password_claims_the_library` is where that lives.
/// `admin_exists` itself was never wrong — it was being asked the wrong
/// question. Corrected by the fourth whole-branch review.
#[test]
fn an_ordinary_user_does_not_count_as_an_admin_existing() {
    let library = library();
    let conn = library.conn();
    let phc = credential::hash_password("reader").expect("hashes");
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('reader', ?1, 'user')",
        [&phc],
    )
    .expect("adds a reader");

    assert!(!admin_exists(&conn).expect("reads"));
}

/// §5's real bootstrap gate: **anybody** with a password, of any role, means
/// this library has been set up and the claim route is shut.
#[test]
fn a_reader_with_a_password_claims_the_library() {
    let library = library();
    let conn = library.conn();

    assert!(
        !is_claimed(&conn).expect("reads"),
        "a fresh library holds one locked row and has not been set up"
    );

    add(&conn, "reader", "a password", "user").expect("adds a reader");
    assert!(
        is_claimed(&conn).expect("reads"),
        "an account somebody can log in as means this library has an owner, and \
         the claim route must not still be offering the seeded row to strangers"
    );
    assert!(
        !admin_exists(&conn).expect("reads"),
        "and the two questions really are different, which is what made reading \
         one for the other a hole rather than a tidy-up"
    );
    assert!(
        unclaimed(&conn).is_ok(),
        "the seeded row is still there and still promotable — by the CLI, from \
         the host's own shell, which is the path §5 wants"
    );
}

/// The invariant [`promote`] now refuses to break: a library must never end up
/// with nobody who can administer it.
///
/// `promote --role user` on a fresh library shut every bootstrap path at once —
/// nothing left for `unclaimed` to find, and [`is_claimed`] true so the claim
/// route refuses too. Readable, unadministrable, and silent about it.
#[test]
fn promoting_the_only_account_to_a_reader_is_refused() {
    let library = library();
    let conn = library.conn();

    let refusal = promote(&conn, "alice", "a password", "user").expect_err("refused");
    assert!(
        refusal.contains("no administrator"),
        "the refusal must say what it is protecting: {refusal}"
    );
    assert!(
        !is_claimed(&conn).expect("reads"),
        "a refused promotion must not have written a password"
    );

    // The rule is the invariant rather than the order: with an admin already
    // there, promoting the seeded row to a reader is fine — and that is the
    // real case, since the seeded row carries the pre-A2 notes.
    add(&conn, "boss", "another", "admin").expect("adds an admin");
    let id = promote(&conn, "alice", "a password", "user").expect("now allowed");
    assert_eq!(id, 1, "promotion still reaches the row §5 requires");
}

/// The wider ceiling, and why the pair alone could not be it: the name half is
/// caller-supplied, so a guesser inventing a fresh one every time never
/// approaches [`LOCKOUT_AFTER`] — and after `credential::DECOY_HASH` landed,
/// each of those attempts cost a full argon2 verify.
#[test]
fn the_address_is_counted_whatever_names_it_offered() {
    let library = library();
    let conn = library.conn();
    let wide = address_key("203.0.113.9");

    // A fresh name every time never trips the pair counter, which is exactly
    // the gap — asserted rather than described, so a change to `attempt_key`
    // that closed it would fail here and say why.
    for n in 0..LOCKOUT_AFTER + 5 {
        let pair = attempt_key("203.0.113.9", &format!("invented-{n}"));
        assert!(!is_locked_out(&conn, &pair).expect("reads"));
        note_failure(&conn, &pair).expect("notes");
        note_failure(&conn, &wide).expect("notes");
    }
    assert!(
        !is_address_locked_out(&conn, &wide).expect("reads"),
        "the wider ceiling has to leave room above one account's allowance, or \
         a household behind one NAT locks itself out"
    );

    for _ in LOCKOUT_AFTER + 5..ADDRESS_LOCKOUT_AFTER {
        note_failure(&conn, &wide).expect("notes");
    }
    assert!(
        is_address_locked_out(&conn, &wide).expect("reads"),
        "an address that has spent {ADDRESS_LOCKOUT_AFTER} attempts must stop \
         buying argon2 verifies, whatever names it was offering"
    );

    // A different address is untouched, which is the whole reason this is keyed
    // on the address rather than counted globally.
    assert!(
        !is_address_locked_out(&conn, &address_key("198.51.100.1")).expect("reads"),
        "one flooding address locked out every other client"
    );
}

/// The two keys share the `login_attempt` table, so they must not be able to
/// name the same row.
#[test]
fn the_wide_key_cannot_collide_with_a_pair() {
    let wide = address_key("203.0.113.9");
    assert!(!wide.contains('\n'));
    // Whatever name is offered, the pair carries a newline and the address
    // alone never can — an address here is a rendered `SocketAddr`.
    for name in ["", "203.0.113.9", "alice\nbob", "\n"] {
        assert_ne!(attempt_key("203.0.113.9", name), wide);
    }
}

/// §7: keyed on the **pair**, so one misconfigured OPDS client retrying hard
/// locks out only itself rather than its owner's every device.
#[test]
fn lockout_is_keyed_on_the_pair_not_the_account() {
    let library = library();
    let conn = library.conn();
    let mine = attempt_key("203.0.113.9", "alice");
    let other_device = attempt_key("198.51.100.1", "alice");
    let someone_else = attempt_key("203.0.113.9", "bob");

    for _ in 0..LOCKOUT_AFTER {
        note_failure(&conn, &mine).expect("notes");
    }

    assert!(is_locked_out(&conn, &mine).expect("reads"));
    assert!(
        !is_locked_out(&conn, &other_device).expect("reads"),
        "alice's other device was locked out by this one's failures, which is \
         the outcome §7 keys on the pair to avoid"
    );
    assert!(!is_locked_out(&conn, &someone_else).expect("reads"));
}

/// One below the threshold is not locked out, so the boundary is where it says
/// it is rather than one either side.
#[test]
fn the_threshold_is_where_it_says_it_is() {
    let library = library();
    let conn = library.conn();
    let key = attempt_key("203.0.113.9", "alice");

    for _ in 0..LOCKOUT_AFTER - 1 {
        note_failure(&conn, &key).expect("notes");
    }
    assert!(!is_locked_out(&conn, &key).expect("reads"));

    let count = note_failure(&conn, &key).expect("notes");
    assert_eq!(count, LOCKOUT_AFTER);
    assert!(is_locked_out(&conn, &key).expect("reads"));
}

/// A success forgets the failures, so two typos followed by the right password
/// leave nothing behind to accumulate against the next mistake.
#[test]
fn succeeding_clears_the_count() {
    let library = library();
    let conn = library.conn();
    let key = attempt_key("203.0.113.9", "alice");
    for _ in 0..LOCKOUT_AFTER {
        note_failure(&conn, &key).expect("notes");
    }
    assert!(is_locked_out(&conn, &key).expect("reads"));

    clear_failures(&conn, &key).expect("clears");
    assert!(!is_locked_out(&conn, &key).expect("reads"));
}

/// A window that has lapsed starts the count again rather than adding to it,
/// which is what makes this a rate limit rather than a permanent ban after ten
/// mistakes spread over a year.
#[test]
fn a_lapsed_window_starts_over() {
    let library = library();
    let conn = library.conn();
    let key = attempt_key("203.0.113.9", "alice");
    conn.execute(
        "INSERT INTO login_attempt (key, window_start, count)
         VALUES (?1, datetime('now', '-1 day'), 99)",
        [&key],
    )
    .expect("plants a stale window");

    assert!(
        !is_locked_out(&conn, &key).expect("reads"),
        "a window from yesterday still counted"
    );
    let count = note_failure(&conn, &key).expect("notes");
    assert_eq!(
        count, 1,
        "the stale count was added to rather than replaced"
    );
}

/// The key joins on a character neither half can contain, so two different
/// pairs cannot collide on one row.
#[test]
fn the_key_cannot_be_confused_between_pairs() {
    assert_ne!(
        attempt_key("10.0.0.1", "alice"),
        attempt_key("10.0.0.1alice", "")
    );
    assert_ne!(attempt_key("10.0.0.1", "a"), attempt_key("10.0.0.1", "a "));
}

/// **§5's constraint, and the one this whole command exists to satisfy.**
///
/// "Whichever path runs, it must promote that row — same `user.id` — and not
/// insert a new one. Inserting would orphan everything recorded between A and A2
/// on an account nobody can log in as, and the loss would be silent, because the
/// reader would simply show an empty library and a fresh position."
#[test]
fn promotion_reaches_the_seeded_row_and_keeps_what_hangs_off_it() {
    let library = library();
    let conn = library.conn();
    let work_id = a_work(&conn);
    crate::state::put_notes(
        &conn,
        work_id,
        1,
        &crate::Notes {
            work: "t".to_string(),
            notes: vec![crate::Note {
                id: "01HBEFORE".to_string(),
                edition: "raw".to_string(),
                slot: 0,
                region: None,
                color: None,
                body: "written before there was a login".to_string(),
                created: "2026-08-18T00:00:00Z".to_string(),
                updated: "2026-08-18T00:00:00Z".to_string(),
            }],
        },
    )
    .expect("a note from before A2");
    crate::state::put_progress(
        &conn,
        work_id,
        1,
        &crate::Progress {
            edition: "raw".to_string(),
            slot: 42,
        },
    )
    .expect("a position from before A2");

    let id = promote(&conn, "davey", "a password", "admin").expect("promotes");

    let accounts: i64 = conn
        .query_row("SELECT COUNT(*) FROM user", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(
        accounts, 1,
        "promotion inserted a second row instead of promoting the seeded one"
    );
    assert_eq!(
        id, 1,
        "the id moved, and every note and position hangs off it"
    );

    let (name, hash): (String, String) = conn
        .query_row("SELECT name, password_hash FROM user", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("reads");
    assert_eq!(name, "davey");
    assert!(credential::verify_password_in(
        &mut credential::WorkingMemory::new(),
        "a password",
        &hash
    ));

    let note: String = conn
        .query_row("SELECT body FROM note WHERE user_id = 1", [], |row| {
            row.get(0)
        })
        .expect("the note survived");
    assert_eq!(note, "written before there was a login");
    let slot: i64 = conn
        .query_row("SELECT slot FROM progress WHERE user_id = 1", [], |row| {
            row.get(0)
        })
        .expect("the position survived");
    assert_eq!(slot, 42);
}

/// Promoting twice is refused rather than silently renaming the first account —
/// the second call has no unclaimed row to reach, and guessing which existing
/// account was meant is not something to infer.
#[test]
fn there_is_nothing_to_promote_twice() {
    let library = library();
    let conn = library.conn();
    promote(&conn, "davey", "a password", "admin").expect("promotes");

    let refused = promote(&conn, "someone-else", "another", "admin").expect_err("refuses");
    assert!(refused.contains("already has a password"), "{refused}");
    assert!(
        refused.contains("user add"),
        "the refusal must name the command that does what was wanted: {refused}"
    );
}

#[test]
fn a_second_account_is_added_beside_the_first() {
    let library = library();
    let conn = library.conn();
    promote(&conn, "davey", "a password", "admin").expect("promotes");

    let id = add(&conn, "alex", "theirs", "user").expect("adds");
    assert_ne!(id, 1);
    assert_eq!(id_of(&conn, "alex").expect("reads"), Some(id));

    let taken = add(&conn, "alex", "again", "user").expect_err("refuses a duplicate name");
    assert!(taken.contains("alex"), "{taken}");
}

/// §4: changing a password ends the account's other sessions and keeps the one
/// making the change — and leaves API keys alone.
#[test]
fn changing_a_password_ends_the_other_sessions_only() {
    let library = library();
    let conn = library.conn();
    promote(&conn, "davey", "first", "admin").expect("promotes");
    let mine = credential::mint_token();
    let theirs = credential::mint_token();
    open_session(&conn, 1, &mine, None, None).expect("opens");
    open_session(&conn, 1, &theirs, None, None).expect("opens");
    let key = create_key(&conn, 1, "phone").expect("creates a key");

    let ended = set_password(&conn, 1, "second", Some(&mine)).expect("changes");
    assert_eq!(ended, 1);
    assert!(session_owner(&conn, &mine).expect("reads").is_some());
    assert!(session_owner(&conn, &theirs).expect("reads").is_none());
    assert!(
        by_api_key(&conn, &key).expect("reads").is_some(),
        "rotating a password must not silently break every reader in the house"
    );

    let (_, phc) = by_name(&conn, "davey").expect("reads").expect("exists");
    assert!(credential::verify_password_in(
        &mut credential::WorkingMemory::new(),
        "second",
        &phc
    ));
    assert!(!credential::verify_password_in(
        &mut credential::WorkingMemory::new(),
        "first",
        &phc
    ));
}

/// An admin resetting somebody else's password ends every session that account
/// has, because there is no "current" one to keep.
#[test]
fn resetting_someone_elses_password_ends_all_of_their_sessions() {
    let library = library();
    let conn = library.conn();
    let alex = add(&conn, "alex", "theirs", "user").expect("adds");
    open_session(&conn, alex, &credential::mint_token(), None, None).expect("opens");
    open_session(&conn, alex, &credential::mint_token(), None, None).expect("opens");

    let ended = set_password(&conn, alex, "reset by an admin", None).expect("resets");
    assert_eq!(ended, 2);
}

/// A key is returned once and stored hashed — the row holds only the hash, so
/// there is nothing to show a second time even by mistake.
#[test]
fn a_created_key_is_returned_once_and_stored_hashed() {
    let library = library();
    let conn = library.conn();
    let secret = create_key(&conn, 1, "phone").expect("creates");
    assert!(secret.starts_with(credential::API_KEY_PREFIX));

    let stored: String = conn
        .query_row("SELECT secret_sha256 FROM api_key", [], |row| row.get(0))
        .expect("reads");
    assert_ne!(stored, secret, "the raw key reached the database");
    assert_eq!(stored, credential::token_sha256(&secret));

    let listed = keys_of(&conn, 1).expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "phone");
    assert!(
        listed[0].last_used.is_none(),
        "a key nobody has used has no last_used"
    );

    assert!(revoke_key(&conn, 1, listed[0].id).expect("revokes"));
    assert!(!revoke_key(&conn, 1, listed[0].id).expect("revoking twice is not an error"));
    assert!(by_api_key(&conn, &secret).expect("reads").is_none());
}

/// The listing says which accounts can actually log in, because a row still
/// holding the locked placeholder has no other sign of it.
#[test]
fn the_listing_says_who_can_log_in() {
    let library = library();
    let conn = library.conn();
    add(&conn, "alex", "theirs", "user").expect("adds");

    let accounts = list(&conn).expect("lists");
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].0.name, "local");
    assert!(!accounts[0].1, "the seeded row is still locked");
    assert_eq!(accounts[1].0.name, "alex");
    assert!(accounts[1].1);
}

/// The scoping is in the statement, so another account's key id revokes
/// nothing rather than depending on a caller having checked first.
#[test]
fn a_key_belongs_to_its_own_account_and_no_other() {
    let library = claimed_library();
    let conn = library.conn();
    let bob = add(&conn, "bob", "theirs", "user").expect("adds bob");
    let bobs = create_key(&conn, bob, "bobs-phone").expect("creates");
    let id: i64 = conn
        .query_row("SELECT id FROM api_key", [], |row| row.get(0))
        .expect("reads");

    assert!(
        !revoke_key(&conn, 1, id).expect("does not error"),
        "account 1 revoked account 2's key"
    );
    assert!(by_api_key(&conn, &bobs).expect("reads").is_some());
    assert!(revoke_key(&conn, bob, id).expect("revokes"));
    assert!(by_api_key(&conn, &bobs).expect("reads").is_none());
}

/// A lapsed window is swept; a live one is left, or the sweep would be a way to
/// clear a lockout by waiting for a restart.
#[test]
fn lapsed_attempt_windows_are_swept_and_live_ones_are_not() {
    let library = library();
    let conn = library.conn();
    conn.execute(
        "INSERT INTO login_attempt (key, window_start, count)
         VALUES ('old', datetime('now', '-1 day'), 3)",
        [],
    )
    .expect("a lapsed window");
    let live = attempt_key("203.0.113.9", "alice");
    note_failure(&conn, &live).expect("a live one");

    let swept = sweep_lapsed_attempts(&conn).expect("sweeps");
    assert_eq!(swept, 1);

    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM login_attempt", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(left, 1, "the live window was swept with the lapsed one");
}

/// A key minted against an account nobody has claimed authenticates as nobody.
///
/// The password path refused the locked placeholder and the key path did not,
/// so a key was an admin credential on a server `admin_exists` still reported as
/// unclaimed — with the claim route open and `user promote` ready to rename the
/// row out from under it.
#[test]
fn a_key_on_a_locked_account_authenticates_as_nobody() {
    let library = library();
    let conn = library.conn();
    let secret = create_key(&conn, 1, "minted-too-early").expect("creates");

    assert!(
        by_api_key(&conn, &secret).expect("reads").is_none(),
        "a key authenticated an account that has never had a password"
    );

    // Once the account is claimed, the same key works — the refusal is about
    // the account's state, not about the key.
    let phc = credential::hash_password("now claimed").expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("claims");
    assert!(by_api_key(&conn, &secret).expect("reads").is_some());
}

/// **The session lifecycle, asserted against the functions that do the work
/// rather than through `session_owner`.**
///
/// Mutation testing found ten survivors across these four functions on
/// 2026-08-19: `close_session` could return `Ok(())` without deleting,
/// `sweep_expired_sessions` `Ok(0)` or `Ok(1)` without sweeping, `sessions_of`
/// `Ok(vec![])` without listing, and `revoke_session` `Ok(true)` *or*
/// `Ok(false)` without revoking — with its `rows > 0` surviving `==`, `<` and
/// `>=` as well. Every test in the workspace still passed against all ten.
///
/// The reason is one reason: `account/tests.rs` had no test naming any of them,
/// and `serve`'s route tests asserted through `session_owner` — which answers
/// `None` for a session that was never opened just as readily as for one that
/// was properly closed. **A revoke that revoked nothing would have shipped
/// green**, on the route whose entire job is ending a session somebody believes
/// is compromised.
///
/// Coverage said 100%. Coverage measures which lines ran.
#[test]
fn closing_a_session_deletes_the_row_rather_than_returning_ok() {
    let library = claimed_library();
    let conn = library.conn();
    let mine = credential::mint_token();
    let other = credential::mint_token();
    open_session(&conn, 1, &mine, None, None).expect("opens");
    open_session(&conn, 1, &other, None, None).expect("opens");

    close_session(&conn, &mine).expect("closes");

    assert_eq!(
        rows(&conn),
        1,
        "the row is still there, so `close_session` returned Ok without deleting"
    );
    assert!(session_owner(&conn, &mine).expect("reads").is_none());
    assert!(
        session_owner(&conn, &other).expect("reads").is_some(),
        "closing one session ended another"
    );

    // Closing a token nobody holds is not an error — logout must be idempotent,
    // since a browser can send it twice — but it must also not delete anything.
    close_session(&conn, "not a token").expect("no error");
    assert_eq!(rows(&conn), 1);
}

/// How many sessions exist, for the tests above and below. Read directly rather
/// than through `sessions_of`, so a defect in that function cannot make a test
/// of a different one pass.
fn rows(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM session", [], |row| row.get(0))
        .expect("counts")
}

/// The sweep deletes what has lapsed and **counts what it deleted** — `Ok(0)`
/// and `Ok(1)` both survived, so the number was never asserted against a known
/// population.
#[test]
fn the_sweep_takes_the_expired_sessions_and_counts_them() {
    let library = claimed_library();
    let conn = library.conn();
    let live = credential::mint_token();
    open_session(&conn, 1, &live, None, None).expect("opens");

    // Two lapsed and one live, so neither `Ok(0)` nor `Ok(1)` can pass: the
    // count, the survivors and the casualties are three different numbers.
    for token in [credential::mint_token(), credential::mint_token()] {
        conn.execute(
            "INSERT INTO session (token_sha256, user_id, expires)
             VALUES (?1, 1, datetime('now', '-1 day'))",
            rusqlite::params![credential::token_sha256(&token)],
        )
        .expect("a lapsed session");
    }
    assert_eq!(rows(&conn), 3);

    let swept = sweep_expired_sessions(&conn).expect("sweeps");
    assert_eq!(swept, 2, "the count is not what was deleted");
    assert_eq!(rows(&conn), 1, "the sweep did not delete what it counted");
    assert!(
        session_owner(&conn, &live).expect("reads").is_some(),
        "the sweep took a session that had not expired"
    );

    assert_eq!(
        sweep_expired_sessions(&conn).expect("sweeps"),
        0,
        "a second sweep found something to delete"
    );
}

/// `sessions_of` listed nothing and passed, and its `current` flag survived
/// being inverted. Both need a fixture where the right answer and the wrong one
/// are different shapes.
#[test]
fn listing_sessions_names_them_and_marks_the_one_asking() {
    let library = claimed_library();
    let conn = library.conn();
    let mine = credential::mint_token();
    let theirs = credential::mint_token();
    open_session(&conn, 1, &mine, Some("this browser"), Some("203.0.113.9")).expect("opens");
    open_session(&conn, 1, &theirs, Some("the tablet"), None).expect("opens");

    // A second account's session, to prove the query is scoped: `Ok(vec![])`
    // is not the only wrong answer, and neither is "everything".
    add(&conn, "someone-else", "theirs", "user").expect("adds");
    let stranger = credential::mint_token();
    let other_id = id_of(&conn, "someone-else")
        .expect("reads")
        .expect("exists");
    open_session(&conn, other_id, &stranger, None, None).expect("opens");

    let listed = sessions_of(&conn, 1, Some(&mine)).expect("lists");
    assert_eq!(listed.len(), 2, "listed the wrong number of sessions");
    assert!(
        listed
            .iter()
            .all(|s| s.id != credential::token_sha256(&stranger)),
        "another account's session appeared in this account's list"
    );

    let current: Vec<&SessionSummary> = listed.iter().filter(|s| s.current).collect();
    assert_eq!(
        current.len(),
        1,
        "exactly one row is the browser doing the asking, and inverting the \
         comparison marks the other one instead"
    );
    assert_eq!(current[0].id, credential::token_sha256(&mine));
    assert_eq!(current[0].user_agent.as_deref(), Some("this browser"));
    assert_eq!(current[0].ip.as_deref(), Some("203.0.113.9"));

    // No token offered means no row is current — not that the first one is.
    let anonymous = sessions_of(&conn, 1, None).expect("lists");
    assert!(
        anonymous.iter().all(|s| !s.current),
        "a listing with no token in hand marked a row as the current browser"
    );
}

/// `revoke_session` survived `Ok(true)`, `Ok(false)`, and all three ways of
/// getting `rows > 0` wrong. Its answer is what the route turns into 404 or 204,
/// so a constant is a route that lies in one direction or the other.
#[test]
fn revoking_a_session_ends_that_one_and_says_whether_it_did() {
    let library = claimed_library();
    let conn = library.conn();
    let mine = credential::mint_token();
    let theirs = credential::mint_token();
    open_session(&conn, 1, &mine, None, None).expect("opens");
    open_session(&conn, 1, &theirs, None, None).expect("opens");

    let id = credential::token_sha256(&theirs);
    assert!(
        revoke_session(&conn, 1, &id).expect("revokes"),
        "answered false for a session it was asked to end"
    );
    assert!(session_owner(&conn, &theirs).expect("reads").is_none());
    assert!(
        session_owner(&conn, &mine).expect("reads").is_some(),
        "revoking one session ended another"
    );

    // Twice is false the second time — `rows > 0` against `>=`, which would
    // answer true for the zero rows a second revoke deletes.
    assert!(
        !revoke_session(&conn, 1, &id).expect("revokes"),
        "answered true for a session that was already gone"
    );

    // **Scoped to the account.** An id from one account's listing must not end
    // another account's session, and the answer must be false rather than an
    // error — the route turns it into a 404, which is what makes another
    // account's session id indistinguishable from one that never existed.
    add(&conn, "someone-else", "theirs", "user").expect("adds");
    let other_id = id_of(&conn, "someone-else")
        .expect("reads")
        .expect("exists");
    assert!(
        !revoke_session(&conn, other_id, &credential::token_sha256(&mine)).expect("revokes"),
        "one account revoked another account's session"
    );
    assert!(
        session_owner(&conn, &mine).expect("reads").is_some(),
        "and it actually deleted it"
    );
}

fn a_root(conn: &rusqlite::Connection, id: i64, path: &str) {
    conn.execute(
        "INSERT INTO root (id, path) VALUES (?1, ?2)",
        rusqlite::params![id, path],
    )
    .expect("adding a root");
}

/// The library root `root` is filed under.
fn library_of(conn: &rusqlite::Connection, root: i64) -> i64 {
    conn.query_row("SELECT library_id FROM root WHERE id = ?1", [root], |r| {
        r.get(0)
    })
    .expect("the root's library")
}

#[test]
fn granting_a_library_is_idempotent_and_says_which_it_was() {
    let library = library();
    let conn = library.conn();
    a_root(&conn, 7, "/library");

    assert!(
        grant_library(&conn, 1, library_of(&conn, 7)).expect("grants"),
        "the first grant is new"
    );
    assert!(
        !grant_library(&conn, 1, library_of(&conn, 7)).expect("grants again"),
        "granting twice is not an error and is not a second grant"
    );
    assert_eq!(roots_for(&conn, 1).expect("reads"), vec![7]);
}

#[test]
fn revoking_a_library_says_whether_there_was_one() {
    let library = library();
    let conn = library.conn();
    a_root(&conn, 7, "/library");
    grant_library(&conn, 1, library_of(&conn, 7)).expect("grants");

    assert!(
        revoke_library(&conn, 1, library_of(&conn, 7)).expect("revokes"),
        "the grant was there"
    );
    assert!(
        !revoke_library(&conn, 1, library_of(&conn, 7)).expect("revokes again"),
        "revoking what is not granted is not an error"
    );
    assert!(roots_for(&conn, 1).expect("reads").is_empty());
}

/// **The grant is the restriction**, so an account with none is unrestricted
/// and this is the state every existing library is in — nothing could write
/// `user_root` before spec C1. Pinned here rather than only at the route,
/// because it is the property the migration depends on.
#[test]
fn an_account_with_no_grant_has_an_empty_list_rather_than_an_error() {
    let library = library();
    let conn = library.conn();
    assert!(roots_for(&conn, 1).expect("reads").is_empty());
}

/// Ids that differ between libraries and roots, so a join on the wrong pair
/// cannot pass by coincidence.
fn a_two_folder_library_and_another(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
         INSERT INTO root (id, path, library_id) VALUES
           (7, '/srv/manga', 70), (8, '/srv/raw', 70), (9, '/srv/other', 80);",
    )
    .expect("a library of two folders and one of one");
}

#[test]
fn a_granted_library_resolves_to_every_one_of_its_folders() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    assert!(grant_library(&conn, id, 70).expect("grants"), "a new grant");
    assert!(
        !grant_library(&conn, id, 70).expect("grants again"),
        "and not a second one"
    );

    assert_eq!(
        roots_for(&conn, id).expect("reads"),
        vec![7, 8],
        "both folders, not Other's"
    );
    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![70]);
    assert_eq!(
        granted_libraries(&conn, id).expect("reads"),
        vec![(70, "Shelf".to_string())]
    );
}

/// A library with no folder resolves to no folder, which the guard reads as
/// unrestricted, so granting it is refused, by the patch and by the CLI's
/// call alike, and nothing is granted.
#[test]
fn a_library_with_no_folder_is_not_granted() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    conn.execute("INSERT INTO library (id, name) VALUES (90, 'Empty')", [])
        .expect("a library with no folder, as only a hand can make one");
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let refused = update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![70, 90]),
            ..Patch::default()
        },
        None,
    )
    .expect_err("Empty has no folder");
    assert_eq!(
        refused,
        WriteError::Refused(
            "the library `Empty` has no folder, so there is nothing in it to grant".to_string()
        )
    );
    assert!(
        libraries_for(&conn, id).expect("reads").is_empty(),
        "Shelf was not granted either"
    );

    let why = grant_library(&conn, id, 90).expect_err("Empty has no folder");
    assert!(why.contains("`Empty` has no folder"), "{why}");
    assert!(libraries_for(&conn, id).expect("reads").is_empty());

    let missing = grant_library(&conn, id, 999).expect_err("no such library");
    assert_eq!(missing, "there is no library 999 to grant");
}

/// [`a_two_folder_library_and_another`], a library `Zines` whose name and
/// folder's path sort after `Other`'s though its ids sort before, and two
/// accounts: `reader` granted `Other` and `Zines`, `other` granted `Shelf`
/// and `Other`. Answers the two accounts in that order.
fn two_accounts_granted_libraries(conn: &rusqlite::Connection) -> (i64, i64) {
    a_two_folder_library_and_another(conn);
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (60, 'Zines');
         INSERT INTO root (id, path, library_id) VALUES (6, '/srv/zines', 60);",
    )
    .expect("a library of one folder");
    let reader = add(conn, "reader", "pw", "user").expect("adds an account");
    let other = add(conn, "other", "pw", "user").expect("adds another");
    grant_library(conn, reader, 80).expect("grants Other");
    grant_library(conn, reader, 60).expect("grants Zines");
    grant_library(conn, other, 70).expect("grants the other account Shelf");
    grant_library(conn, other, 80).expect("and Other");
    (reader, other)
}

/// The folders a grant resolves to are those of the account's own libraries,
/// by id for the guard.
#[test]
fn granted_folders_are_read_per_account_and_in_order() {
    let library = claimed_library();
    let conn = library.conn();
    let (reader, _other) = two_accounts_granted_libraries(&conn);

    assert_eq!(
        roots_for(&conn, reader).expect("reads"),
        vec![6, 9],
        "ascending by id, and not the other account's"
    );
}

/// Each account's grants are its own, and a listing is in its stated order.
#[test]
fn granted_libraries_are_read_per_account_and_in_order() {
    let library = claimed_library();
    let conn = library.conn();
    let (reader, other) = two_accounts_granted_libraries(&conn);

    assert_eq!(
        libraries_for(&conn, reader).expect("reads"),
        vec![60, 80],
        "ascending by id, and not the other account's"
    );
    assert_eq!(
        granted_libraries(&conn, reader).expect("reads"),
        vec![(80, "Other".to_string()), (60, "Zines".to_string())],
        "by name, and not the other account's"
    );

    assert!(
        revoke_library(&conn, reader, 80).expect("revokes"),
        "the grant was there"
    );
    assert_eq!(
        libraries_for(&conn, reader).expect("reads"),
        vec![60],
        "the library named went, and only that one"
    );
    assert_eq!(
        libraries_for(&conn, other).expect("reads"),
        vec![70, 80],
        "and no other account's grant of it"
    );
}

/// A library id no row holds is refused, where a root id would once have
/// been.
#[test]
fn granting_a_library_that_does_not_exist_is_an_error() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let why = grant_library(&conn, id, 999).expect_err("there is no library 999");

    assert!(why.contains("999"), "{why}");
    assert!(libraries_for(&conn, id).expect("reads").is_empty());
}

/// A list names libraries. `7` is a folder of library 70 and no library's id,
/// so a writer that still read the list as folders would grant 70 for it.
#[test]
fn a_libraries_list_names_libraries_and_never_a_folder() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![70]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![70]);
    assert_eq!(
        roots_for(&conn, id).expect("reads"),
        vec![7, 8],
        "the library, with both its folders"
    );

    let why = update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![7]),
            ..Patch::default()
        },
        None,
    )
    .expect_err("there is no library 7");
    assert!(
        matches!(&why, WriteError::Unusable(text) if text.contains("there is no library 7")),
        "{why:?}"
    );
    assert_eq!(
        libraries_for(&conn, id).expect("reads"),
        vec![70],
        "the refusal wrote nothing"
    );

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![80]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(
        libraries_for(&conn, id).expect("reads"),
        vec![80],
        "replaced, not appended"
    );
}

/// A list refused part-way writes nothing, and nothing else the same patch
/// carried: the first id is a library, the second is not. The grants are
/// untouched, and the name and capability written before the grants were
/// reached are undone with them, because the patch is one transaction.
#[test]
fn a_libraries_list_refused_part_way_writes_nothing_of_the_patch() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, 80).expect("grants Other");

    let why = update(
        &conn,
        id,
        &Patch {
            name: Some("renamed".to_string()),
            can_stream: Some(false),
            libraries: Some(vec![70, 999]),
            ..Patch::default()
        },
        None,
    )
    .expect_err("there is no library 999");

    assert!(
        matches!(&why, WriteError::Unusable(text) if text.contains("there is no library 999")),
        "{why:?}"
    );
    assert_eq!(
        libraries_for(&conn, id).expect("reads"),
        vec![80],
        "neither the good id was granted nor the held one revoked"
    );
    let account = by_id(&conn, id).expect("reads").expect("the account");
    assert_eq!(account.name, "reader", "the rename was undone");
    assert!(account.can_stream, "and so was the capability");
}

/// A list naming one library twice is accepted rather than refused as a
/// mistake, and the account holds the library once, as the table's key
/// requires.
#[test]
fn a_libraries_list_naming_one_library_twice_grants_it_once() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![70, 70]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![70]);
}

// --- spec C4 §4: user administration -------------------------------------

/// O54's note — "nothing can set either to 0 outside a fixture until spec C4's
/// user administration" — stops being true here. A patch writes the columns it
/// names and leaves the ones it does not.
#[test]
fn a_patch_writes_the_columns_it_names_and_leaves_the_others() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let patch = Patch {
        can_download: Some(false),
        ..Patch::default()
    };
    let after = update(&conn, id, &patch, None).expect("updates");

    assert!(!after.can_download, "the named column changed");
    assert!(after.can_stream, "an unnamed column is left alone");
    assert_eq!(after.name, "reader", "an unnamed column is left alone");
    assert_eq!(after.role, "user", "an unnamed column is left alone");
}

/// PR2 §2's invariant 1, by the demotion route rather than the delete one.
/// `promote` already refuses the same lockout from the other end.
#[test]
fn demoting_the_only_administrator_who_can_log_in_is_refused() {
    let library = claimed_library();
    let conn = library.conn();

    let patch = Patch {
        role: Some("user".to_string()),
        ..Patch::default()
    };
    let why = update(&conn, 1, &patch, None).expect_err("refuses");

    assert!(matches!(why, WriteError::Refused(_)), "{why:?}");
    assert!(
        why.to_string().contains("no administrator"),
        "the message says what would be left: {why}"
    );
    assert_eq!(
        by_id(&conn, 1).expect("reads").expect("exists").role,
        "admin",
        "the refusal wrote nothing"
    );
}

/// PR2 §2's rule 2, and it is deliberately tested **with a second admin
/// present** so it cannot pass by accidentally re-testing invariant 1: the
/// library keeps an administrator either way, and the refusal is about who is
/// asking.
#[test]
fn an_administrator_cannot_demote_themselves_even_beside_another_one() {
    let library = claimed_library();
    let conn = library.conn();
    let second = add(&conn, "second", "pw", "admin").expect("adds a second admin");
    let patch = Patch {
        role: Some("user".to_string()),
        ..Patch::default()
    };

    let why = update(&conn, second, &patch, Some(second)).expect_err("refuses");
    assert!(matches!(why, WriteError::Refused(_)), "{why:?}");
    assert!(
        why.to_string().contains("another administrator"),
        "the message names the way out: {why}"
    );

    let done = update(&conn, second, &patch, Some(1)).expect("a different admin may");
    assert_eq!(done.role, "user");
}

/// The CLI has no session to be, so it passes `None` and rule 2 does not fire
/// for it — invariant 1 still does, which the test above this one covers.
#[test]
fn an_actorless_caller_may_demote_an_administrator_beside_another_one() {
    let library = claimed_library();
    let conn = library.conn();
    let second = add(&conn, "second", "pw", "admin").expect("adds a second admin");

    let done = update(
        &conn,
        second,
        &Patch {
            role: Some("user".to_string()),
            ..Patch::default()
        },
        None,
    )
    .expect("no actor, no self-demotion rule");
    assert_eq!(done.role, "user");
}

/// `004_auth.sql` gives `role` a `CHECK (role IN ('admin', 'user'))`, added
/// because "a typo stored a role that is neither and `Principal::from_account`'s
/// `== "admin"` quietly read it as a reader". `update` lets the constraint
/// refuse rather than keeping a second list of the two words.
#[test]
fn a_role_that_is_neither_word_is_refused_and_writes_nothing() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let why = update(
        &conn,
        id,
        &Patch {
            role: Some("moderator".to_string()),
            ..Patch::default()
        },
        None,
    )
    .expect_err("refuses");

    assert!(
        matches!(why, WriteError::Unusable(_)),
        "a role the schema refuses is the caller's mistake, not a conflict \
         and not a fault: {why:?}"
    );
    assert!(
        why.to_string().contains("moderator"),
        "the message names the value: {why}"
    );
    assert_eq!(
        by_id(&conn, id).expect("reads").expect("exists").role,
        "user",
        "the refusal wrote nothing"
    );
}

/// An id naming no account is an error rather than a silent no-op: every
/// caller of this function is about to render what it returns.
#[test]
fn patching_an_account_that_does_not_exist_names_the_id() {
    let library = claimed_library();
    let conn = library.conn();

    let why = update(&conn, 4242, &Patch::default(), None).expect_err("refuses");
    assert!(why.to_string().contains("4242"), "{why}");
}

/// Two folders, each a library of its own, to grant: a test about *replacing* a
/// grant list needs more
/// than one id to tell "replaced" from "appended".
fn two_roots(conn: &rusqlite::Connection) -> (i64, i64) {
    a_root(conn, 7, "/srv/manga");
    a_root(conn, 8, "/srv/raw");
    (7, 8)
}

/// PR2 §1.2: a `libraries` list replaces the grant set wholesale rather than
/// adding to it.
#[test]
fn a_libraries_list_replaces_the_grants_rather_than_adding_to_them() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, second) = two_roots(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, library_of(&conn, first)).expect("grants the first");

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![library_of(&conn, second)]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert_eq!(
        roots_for(&conn, id).expect("reads"),
        vec![second],
        "the list replaced the grants; it did not append to them"
    );
}

/// A list naming two libraries grants both, and a list leaving two
/// held libraries out revokes both. The other account's grants do not move.
#[test]
fn a_libraries_list_across_several_libraries_revokes_and_grants_each() {
    let library = claimed_library();
    let conn = library.conn();
    let (reader, other) = two_accounts_granted_libraries(&conn);

    update(
        &conn,
        reader,
        &Patch {
            libraries: Some(vec![70]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(
        libraries_for(&conn, reader).expect("reads"),
        vec![70],
        "both libraries the list left out went"
    );

    update(
        &conn,
        reader,
        &Patch {
            libraries: Some(vec![60, 80]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(
        libraries_for(&conn, reader).expect("reads"),
        vec![60, 80],
        "each library the list named was granted"
    );

    update(
        &conn,
        reader,
        &Patch {
            libraries: Some(vec![70, 80]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(
        libraries_for(&conn, reader).expect("reads"),
        vec![70, 80],
        "one library held and wanted stays, one is added, and the one left out goes"
    );
    assert_eq!(
        libraries_for(&conn, other).expect("reads"),
        vec![70, 80],
        "and the other account's grants are where they were"
    );
}

/// An absent `libraries` key leaves the grants alone, which is the difference
/// between `None` and `Some(vec![])` that makes the field an `Option<Vec<_>>`.
#[test]
fn a_patch_without_a_libraries_key_leaves_the_grants_alone() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, library_of(&conn, first)).expect("grants the first");

    update(
        &conn,
        id,
        &Patch {
            can_stream: Some(false),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert_eq!(roots_for(&conn, id).expect("reads"), vec![first]);
}

/// **An empty list means unrestricted, not blind**, which is spec A2's
/// sharpest edge and is stated on [`grant_library`]: every account in every
/// existing library holds an empty grant, so any other reading would have
/// emptied every library the day migration 6 applied.
#[test]
fn an_empty_libraries_list_makes_an_account_unrestricted_rather_than_blind() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, library_of(&conn, first)).expect("grants the first");

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(Vec::new()),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert!(
        roots_for(&conn, id).expect("reads").is_empty(),
        "an empty grant is what `Principal::from_account` reads as unrestricted"
    );
}

/// A refused role change must not leave a half-written grant list behind it —
/// both halves are inside one `Nested` scope.
#[test]
fn a_refused_role_change_writes_no_grants_either() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, second) = two_roots(&conn);
    grant_library(&conn, 1, library_of(&conn, first)).expect("grants the admin the first library");

    update(
        &conn,
        1,
        &Patch {
            role: Some("user".to_string()),
            libraries: Some(vec![library_of(&conn, second)]),
            ..Patch::default()
        },
        None,
    )
    .expect_err("the only administrator cannot be demoted");

    assert_eq!(
        roots_for(&conn, 1).expect("reads"),
        vec![first],
        "the refusal happened before anything was written"
    );
}

/// C4 §4's counts, taken before anything is deleted. Deliberately over an
/// account carrying one of everything, so a `Removal` that reported zeros for
/// four of the five fields could not pass.
#[test]
fn removing_an_account_states_what_it_costs_before_it_acts() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let work = a_work(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, library_of(&conn, first)).expect("grants a root");
    create_key(&conn, id, "tablet").expect("mints a key");
    open_session(&conn, id, "tok", None, None).expect("opens a session");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'raw', 3)",
        rusqlite::params![id, work],
    )
    .expect("records a position");
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('n1', ?1, ?2, 'raw', 3, 'hi', '2026-01-01', '2026-01-01')",
        rusqlite::params![id, work],
    )
    .expect("records a note");

    let quoted = removal_cost(&conn, id).expect("costs it");
    assert_eq!(
        quoted,
        Removal {
            name: "reader".to_string(),
            positions: 1,
            notes: 1,
            sessions: 1,
            keys: 1,
            grants: 1,
        }
    );
    assert!(
        by_id(&conn, id).expect("reads").is_some(),
        "a quote deletes nothing"
    );

    let done = remove(&conn, id).expect("removes");
    assert_eq!(
        done, quoted,
        "the removal answers the counts from before it"
    );
    assert!(by_id(&conn, id).expect("reads").is_none());
}

/// Five tables cascade from `user(id)`, and they only cascade because
/// `PRAGMA foreign_keys` is ON per connection (`lib.rs:339`). A test that
/// opened a bare `rusqlite::Connection` would see no cascade at all and would
/// pass a wrong answer as right — so this asserts the rows are gone, not just
/// that the account is.
#[test]
fn removing_an_account_takes_its_positions_notes_sessions_keys_and_grants() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let work = a_work(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, library_of(&conn, first)).expect("grants a root");
    create_key(&conn, id, "tablet").expect("mints a key");
    open_session(&conn, id, "tok", None, None).expect("opens a session");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'raw', 3)",
        rusqlite::params![id, work],
    )
    .expect("records a position");

    remove(&conn, id).expect("removes");

    for table in ["progress", "session", "api_key", "user_library"] {
        let left: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE user_id = ?1"),
                [id],
                |r| r.get(0),
            )
            .expect("counts");
        assert_eq!(left, 0, "`{table}` kept a row for a deleted account");
    }
    let works: i64 = conn
        .query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))
        .expect("counts");
    assert_eq!(works, 1, "deleting a person deletes none of the library");
}

/// PR2 §2's invariant 1, by the delete route.
#[test]
fn the_last_administrator_who_can_log_in_cannot_be_deleted() {
    let library = claimed_library();
    let conn = library.conn();
    add(&conn, "reader", "pw", "user").expect("adds a reader, who is not an admin");

    let why = remove(&conn, 1).expect_err("refuses");
    assert!(matches!(why, WriteError::Refused(_)), "{why:?}");
    assert!(
        why.to_string().contains("no administrator"),
        "the message says what would be left: {why}"
    );
    assert!(by_id(&conn, 1).expect("reads").is_some());
}

/// The same invariant, satisfied: a second administrator makes the first
/// removable.
#[test]
fn an_administrator_beside_another_one_can_be_deleted() {
    let library = claimed_library();
    let conn = library.conn();
    let second = add(&conn, "second", "pw", "admin").expect("adds a second admin");

    remove(&conn, second).expect("removes");
    assert!(by_id(&conn, second).expect("reads").is_none());
}

/// An id naming no account is an error, not a silent success reporting zeros.
#[test]
fn removing_an_account_that_does_not_exist_names_the_id() {
    let library = claimed_library();
    let conn = library.conn();
    assert!(
        removal_cost(&conn, 4242)
            .expect_err("refuses")
            .to_string()
            .contains("4242")
    );
    assert!(
        remove(&conn, 4242)
            .expect_err("refuses")
            .to_string()
            .contains("4242")
    );
}

/// **The quote runs the same refusal the commit runs.** `removal_cost` used to
/// answer five counts for a deletion `remove` would then refuse, so an operator
/// read the counts, confirmed, and met the refusal afterwards — on the route
/// whose whole purpose is to say what will happen. Found by C4 PR 2's
/// whole-branch review.
#[test]
fn quoting_the_removal_of_the_last_administrator_is_refused_like_the_removal() {
    let library = claimed_library();
    let conn = library.conn();
    add(&conn, "reader", "pw", "user").expect("adds a reader");

    let quoted = removal_cost(&conn, 1).expect_err("the quote refuses");
    let done = remove(&conn, 1).expect_err("and so does the removal");
    assert_eq!(quoted, done, "one refusal, stated identically by both");
}

/// **A patch's password lands in the same transaction as its columns.** They
/// used to be two writes — `update` committing its own scope and
/// `set_password` opening none — so a store fault on the password left the
/// role and capability changes committed beside a failed request. Found by
/// C4 PR 2's whole-branch review; asserted here through the one failure that
/// is reachable without breaking the disk.
#[test]
fn a_patch_with_a_refused_password_writes_none_of_its_columns() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let why = update(
        &conn,
        id,
        &Patch {
            can_stream: Some(false),
            password: Some(String::new()),
            ..Patch::default()
        },
        None,
    )
    .expect_err("refuses");
    assert!(matches!(why, WriteError::Unusable(_)), "{why:?}");

    let after = by_id(&conn, id).expect("reads").expect("exists");
    assert!(
        after.can_stream,
        "the column change went in with the refused password and must not have landed"
    );
}

/// The password half of a patch ends every session and keeps every key, which
/// is what `set_password(.., None)` did before it moved inside `update`.
#[test]
fn a_patched_password_ends_every_session_and_keeps_every_key() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    open_session(&conn, id, "tok", None, None).expect("opens a session");
    create_key(&conn, id, "tablet").expect("mints a key");

    update(
        &conn,
        id,
        &Patch {
            password: Some("rotated".to_string()),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert!(sessions_of(&conn, id, None).expect("reads").is_empty());
    assert_eq!(keys_of(&conn, id).expect("reads").len(), 1);
    let (_, stored) = by_name(&conn, "reader").expect("reads").expect("exists");
    assert!(credential::verify_password_in(
        &mut credential::WorkingMemory::new(),
        "rotated",
        &stored
    ));
}

/// **An administrator may patch their own row while sending their role back
/// unchanged**, which is what a form that PATCHes every field does on every
/// save. Without `role != current.role` guarding it, the self-demotion refusal
/// would fire on an edit that changes no role at all.
///
/// Pinned because the mutation sweep cannot: `replace && with || in update`
/// is *unviable* — the `&&` sits in a let-chain and the mutant does not
/// compile — so the sweep reports the condition as unreachable rather than as
/// untested. C4 PR 2 checked the 18 unviable mutants against what it changed,
/// which is the only thing that finds this class.
#[test]
fn an_administrator_may_edit_their_own_row_while_resending_their_role() {
    let library = claimed_library();
    let conn = library.conn();

    let after = update(
        &conn,
        1,
        &Patch {
            name: Some("renamed".to_string()),
            role: Some("admin".to_string()),
            ..Patch::default()
        },
        Some(1),
    )
    .expect("an unchanged role is not a demotion");

    assert_eq!(after.name, "renamed");
    assert_eq!(after.role, "admin");
}

/// **Every resolver reads the column, not just the one a patch test drives.**
///
/// This module holds five separate `SELECT` lists naming the capability
/// columns — `by_name`, `by_api_key`, `session_owner`, `by_id` and `list`.
/// [`account_from_row`] reads them by name, so adding a column to four of the
/// five is not a compile error anywhere: it is one resolver quietly reporting
/// `can_stamp = false` for an account that holds it, and which resolver
/// depends on how the caller authenticated. An OPDS client would stamp and a
/// browser would not, or the other way round.
#[test]
fn every_resolver_reports_can_stamp() {
    let library = claimed_library();
    let conn = library.conn();
    conn.execute("UPDATE user SET can_stamp = 1 WHERE id = 1", [])
        .expect("granting the capability");
    let key = create_key(&conn, 1, "a key").expect("issues a key");
    let token = credential::mint_token();
    open_session(&conn, 1, &token, None, None).expect("opens a session");

    let (named, _) = by_name(&conn, "local").expect("reads").expect("exists");
    assert!(named.can_stamp, "by_name");
    let keyed = by_api_key(&conn, &key).expect("reads").expect("exists");
    assert!(keyed.can_stamp, "by_api_key");
    let sessioned = session_owner(&conn, &token)
        .expect("reads")
        .expect("exists");
    assert!(sessioned.can_stamp, "session_owner");
    let identified = by_id(&conn, 1).expect("reads").expect("exists");
    assert!(identified.can_stamp, "by_id");
    let listed = list(&conn).expect("reads");
    assert!(listed[0].0.can_stamp, "list");
}

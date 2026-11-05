//! Row-level account operations: who an incoming credential belongs to, and
//! what that account may see.
//!
//! **Here rather than in `taiyaku-serve`** because `taiyaku-cli` needs the same
//! operations for `library user promote` and `library key create`, and
//! `taiyaku-library` is the only crate both depend on. The HTTP *shape* of a
//! refusal is `taiyaku-serve`'s business — the two 401s, the `WWW-Authenticate`
//! header, the cookie — and none of it is here. What is here is the question
//! "whose credential is this", which has one answer wherever it is asked from.
//!
//! Spec A2 is the design; `docs/superpowers/specs/2026-08-14-library-auth-design.md`.

use rusqlite::{Connection, OptionalExtension as _, params};

use crate::credential;

/// One account, **minus its password hash**.
///
/// [`by_name`] returns the hash separately rather than on this struct, so
/// nothing in this crate can carry a hash somewhere it was not expected — a
/// principal held for the life of a request, a log line, a serialised summary.
/// The one caller that needs it needs it for exactly one comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub can_download: bool,
    pub can_stream: bool,
    /// Spec C3 §6. **Not folded into the admin role** by
    /// `taiyaku-serve`'s `Principal::from_account`, unlike the two above —
    /// the one exception to O86, argued in
    /// `docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md`
    /// §1.1.
    pub can_stamp: bool,
}

fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get("id")?,
        name: row.get("name")?,
        role: row.get("role")?,
        can_download: row.get::<_, i64>("can_download")? != 0,
        can_stream: row.get::<_, i64>("can_stream")? != 0,
        can_stamp: row.get::<_, i64>("can_stamp")? != 0,
    })
}

/// The account with this name, and the PHC string stored against it.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn by_name(conn: &Connection, name: &str) -> Result<Option<(Account, String)>, String> {
    conn.query_row(
        "SELECT id, name, role, can_download, can_stream, can_stamp, password_hash
           FROM user
          WHERE name = ?1",
        params![name],
        |row| {
            Ok((
                account_from_row(row)?,
                row.get::<_, String>("password_hash")?,
            ))
        },
    )
    .optional()
    .map_err(|e| format!("looking up the account `{name}`: {e}"))
}

/// The account an API key belongs to, or `None` for a key that is unknown,
/// revoked or expired.
///
/// **One answer for all three**, so a client cannot tell them apart — "this key
/// was real once" and "this key never was" are the same refusal.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn by_api_key(conn: &Connection, secret: &str) -> Result<Option<Account>, String> {
    let hashed = credential::token_sha256(secret);
    let found = conn
        .query_row(
            // **`password_hash <> LOCKED` here as well as on the password
            // path.** A locked account is one nobody has claimed, so a key
            // minted against it would authenticate while the claim route stays
            // open and `user promote` treats the row as unclaimed, renaming it
            // out from under a live credential.
            "SELECT u.id, u.name, u.role, u.can_download, u.can_stream, u.can_stamp
               FROM api_key k
               JOIN user u ON u.id = k.user_id
              WHERE k.secret_sha256 = ?1
                AND u.password_hash <> ?2
                AND (k.expires IS NULL OR k.expires > datetime('now'))",
            params![hashed, credential::LOCKED],
            account_from_row,
        )
        .optional()
        .map_err(|e| format!("looking up an API key: {e}"))?;

    if found.is_some() {
        // Best effort, deliberately: a failed touch must not fail the request
        // it was recording. The credential was already valid, and an account
        // screen missing one `last_used` is a smaller problem than a page that
        // did not load. An account screen where every row says nothing at all,
        // though, is unusable for deciding which key to revoke — which is why
        // it is recorded and not skipped.
        let _ = conn.execute(
            "UPDATE api_key SET last_used = datetime('now') WHERE secret_sha256 = ?1",
            params![hashed],
        );
    }
    Ok(found)
}

/// What an account may see: the libraries it was granted, and the directories
/// they reach.
///
/// Two cases rather than a list where empty means "everything": a list that
/// can stand for every library turns any path that empties it (a revoke, a
/// removal, a library with no directory) into a silent widening. `Only` with no
/// directories sees nothing. No `Default`, so each place that builds one says
/// which case it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// An administrator, or an account set to "Every library", including any
    /// added later.
    Every,
    /// Only these, both ascending. **Two lists, because grants and reach
    /// answer different questions** (overlapping-libraries spec §4).
    Only {
        /// The directories (root ids) the granted libraries reach
        /// ([`crate::reach`]): which books the account sees.
        roots: Vec<i64>,
        /// The libraries granted: which libraries it lists and opens, so a
        /// library nested in a granted one is not listed for that (libraries
        /// design §3, "a restricted account sees only granted libraries").
        libraries: Vec<i64>,
    },
}

impl Access {
    /// Whether a directory is inside this access: one the granted libraries
    /// reach.
    pub fn allows(&self, root_id: i64) -> bool {
        match self {
            Access::Every => true,
            Access::Only { roots, .. } => roots.contains(&root_id),
        }
    }

    /// Whether this access was granted `library`, which is what lists it and
    /// opens its page. Reaching one of its directories through another library
    /// does not.
    pub fn grants(&self, library: i64) -> bool {
        match self {
            Access::Every => true,
            Access::Only { libraries, .. } => libraries.contains(&library),
        }
    }

    /// `roots` alone, granting no library: for a test that asks only which
    /// books a set of directories shows.
    #[cfg(any(test, feature = "test-support"))]
    pub fn roots(roots: Vec<i64>) -> Self {
        Access::Only {
            roots,
            libraries: Vec::new(),
        }
    }

    /// The `AND` clause this access adds to a query, with `in_dirs` writing
    /// the test over one `?` per directory: the one place the cases of `Access`
    /// become SQL, for `browse` and for the server's own queries alike.
    ///
    /// **`Only` with no directory is a false literal, never a clause left out**,
    /// because an account granted nothing must match nothing and a missing
    /// clause matches everything. It binds no parameter, as `Every`'s missing
    /// clause binds none, which is why [`Access::bound`] answers both with no
    /// values and every statement's count still agrees with its holes.
    pub fn grant_clause(&self, in_dirs: impl FnOnce(&str) -> String) -> String {
        match self {
            Access::Every => String::new(),
            Access::Only { roots, .. } if roots.is_empty() => " AND 0".to_owned(),
            Access::Only { roots, .. } => {
                format!(" AND {}", in_dirs(&crate::browse::holes(roots.len())))
            }
        }
    }

    /// The values one [`Access::grant_clause`] binds, in hole order.
    pub fn bound(&self) -> &[i64] {
        match self {
            Access::Every => &[],
            Access::Only { roots, .. } => roots,
        }
    }
}

/// What this account may see: every library when its `every_library` is set,
/// and otherwise the libraries it was granted and the directories they reach
/// ([`crate::reach::of_libraries`], from the `root` rows as they are at each call).
///
/// **"Only these" with no grant sees nothing**, never every library, so that
/// revoking an account's last grant or removing its only library leaves it
/// blind rather than widened. An id naming no account reads the same way.
///
/// **A grant counts whether or not its library still has a directory**: it is
/// in `libraries`, and an account whose grants reach no directory sees nothing.
///
/// The role is not read here: `taiyaku-serve`'s `Principal::from_account`
/// gives an administrator `Every` whatever this answers.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn access_for(conn: &Connection, user_id: i64) -> Result<Access, String> {
    let every = conn
        .query_row(
            "SELECT every_library FROM user WHERE id = ?1",
            params![user_id],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(|e| format!("reading whether account {user_id} sees every library: {e}"))?;
    if every == Some(true) {
        return Ok(Access::Every);
    }
    let libraries = libraries_for(conn, user_id)?;
    let roots = crate::scan::roots(conn)?;
    Ok(Access::Only {
        roots: crate::reach::of_libraries(&roots, &libraries),
        libraries,
    })
}

/// The libraries this account was granted, ascending.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn libraries_for(conn: &Connection, user_id: i64) -> Result<Vec<i64>, String> {
    let mut statement = conn
        .prepare("SELECT library_id FROM user_library WHERE user_id = ?1 ORDER BY library_id")
        .map_err(|e| format!("preparing the granted-libraries lookup: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| row.get(0))
        .map_err(|e| format!("reading granted libraries: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| format!("reading granted libraries: {e}"))
}

/// Whether a library has the id.
///
/// **A library with no directory may be granted**: it resolves to no directory, so
/// granting it to an account seeing only some libraries adds nothing, and on
/// one seeing every library it narrows to nothing. Neither widens.
fn library_exists(conn: &Connection, library_id: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM library WHERE id = ?1)",
        params![library_id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(|e| format!("checking whether library {library_id} can be granted: {e}"))
}

/// Grant `user_id` sight of `library_id`: every directory filed under it.
///
/// **Granting is restricting**: the account is set to "only these" and the
/// library is added to them, so an account that saw every library sees only
/// this one afterwards. The answer is `true` when either changed, so a CLI can
/// print the truth rather than a cheerful lie.
///
/// `INSERT OR IGNORE` rather than a probe and an insert: `user_library` is
/// `WITHOUT ROWID` with `(user_id, library_id)` as its key, so the conflict is
/// exactly the case that means "already granted".
///
/// # Errors
///
/// Returns an error if no library has `library_id`, or if a write fails.
pub fn grant_library(conn: &Connection, user_id: i64, library_id: i64) -> Result<bool, String> {
    if !library_exists(conn, library_id)? {
        return Err(format!("there is no library {library_id} to grant"));
    }
    let failed =
        |e: rusqlite::Error| format!("granting library {library_id} to user {user_id}: {e}");
    // One scope, so the account is never left narrowed with the row unwritten.
    let scope = crate::Nested::open(conn, "grant_library").map_err(failed)?;
    let narrowed = conn
        .execute(
            "UPDATE user SET every_library = 0 WHERE id = ?1 AND every_library = 1",
            params![user_id],
        )
        .map_err(failed)?;
    let added = conn
        .execute(
            "INSERT OR IGNORE INTO user_library (user_id, library_id) VALUES (?1, ?2)",
            params![user_id, library_id],
        )
        .map_err(failed)?;
    scope.commit().map_err(failed)?;
    Ok(narrowed + added > 0)
}

/// Take back a grant, reporting whether there was one.
///
/// **`every_library` is left alone**, so revoking an account's last grant
/// leaves it seeing no library. Only [`set_every_library`], or a patch of
/// [`Grant::Every`], lifts the limit.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn revoke_library(conn: &Connection, user_id: i64, library_id: i64) -> Result<bool, String> {
    let changed = conn
        .execute(
            "DELETE FROM user_library WHERE user_id = ?1 AND library_id = ?2",
            params![user_id, library_id],
        )
        .map_err(|e| format!("revoking library {library_id} from user {user_id}: {e}"))?;
    Ok(changed == 1)
}

/// Let `user_id` see every library, including any added later: the setting
/// is raised and the account's grants are deleted, so a later "only these"
/// starts from none rather than from rows left waiting. `true` when either
/// changed.
///
/// # Errors
///
/// Returns an error if a write fails.
pub fn set_every_library(conn: &Connection, user_id: i64) -> rusqlite::Result<bool> {
    let scope = crate::Nested::open(conn, "set_every_library")?;
    let raised = conn.execute(
        "UPDATE user SET every_library = 1 WHERE id = ?1 AND every_library = 0",
        params![user_id],
    )?;
    let deleted = conn.execute(
        "DELETE FROM user_library WHERE user_id = ?1",
        params![user_id],
    )?;
    scope.commit()?;
    Ok(raised + deleted > 0)
}

/// The libraries granted to `user_id`, as `(id, name)`, by name.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn granted_libraries(conn: &Connection, user_id: i64) -> Result<Vec<(i64, String)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT l.id, l.name FROM user_library ul JOIN library l ON l.id = ul.library_id
              WHERE ul.user_id = ?1 ORDER BY l.name",
        )
        .map_err(|e| format!("preparing the granted-libraries listing: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("listing granted libraries: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("listing granted libraries: {e}"))
}

/// Whether any admin can actually log in.
///
/// **Role is not enough, and reading it alone is the bug this doc comment
/// exists to prevent.** Migration 1 seeds `(1, 'local', 'admin')` and migration
/// 4 gives that row [`credential::LOCKED`], so a fresh library has an admin row
/// and no admin. Spec A2 §5's claim route is open only while this answers false;
/// counting the seeded row would close it before anybody could reach it, and the
/// server would be unclaimable by its own owner with no error saying why.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn admin_exists(conn: &Connection) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM user WHERE role = 'admin' AND password_hash <> ?1",
        params![credential::LOCKED],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .map_err(|e| format!("counting admins: {e}"))
}

/// Whether this library has been set up at all — whether **anybody** can log
/// in, of any role.
///
/// **This, and not [`admin_exists`], is what §5's claim route turns on.**
/// `taiyaku library user add alice` is a real way to make the first account and
/// it inserts with role `user`, leaving the seeded row locked — so
/// `admin_exists` answers false on a library its owner has already set up and
/// is already reading from, while `unclaimed` still finds row 1. A claim route
/// gated on `admin_exists` would let a stranger reaching `POST /api/claim` take
/// that row, which §5 requires promotion to update rather than insert beside,
/// and so inherit both administrator and every note and reading position
/// recorded before the library had a login. Pinned by
/// `session::tests::a_library_someone_has_already_set_up_cannot_be_claimed_by_a_stranger`.
///
/// The two functions both remain because they answer different questions and
/// both are asked: this one gates the claim route, the "nobody can log in"
/// startup warning and the `--allow-web-setup` notice, and `admin_exists` is
/// what `promote` asks.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn is_claimed(conn: &Connection) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM user WHERE password_hash <> ?1",
        params![credential::LOCKED],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .map_err(|e| format!("counting accounts with a password: {e}"))
}

/// How long a session lives without being used.
///
/// §4: thirty days, slid forward on use — one login per browser per month,
/// which is what makes §9's refusal of a `--no-auth` bypass cost nothing worth
/// having.
pub const SESSION_DAYS: i64 = 30;

/// Record a new session.
///
/// **Takes the raw token and is the only thing that writes the column**, so no
/// caller can pass the stored form by mistake and no caller has to remember to
/// hash. The raw value leaves here in the `Set-Cookie` header and is never
/// written anywhere.
///
/// # Errors
///
/// Returns an error if the insert fails.
pub fn open_session(
    conn: &Connection,
    user_id: i64,
    token: &str,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO session (token_sha256, user_id, expires, user_agent, ip)
         VALUES (?1, ?2, datetime('now', ?3), ?4, ?5)",
        params![
            credential::token_sha256(token),
            user_id,
            format!("+{SESSION_DAYS} days"),
            user_agent,
            ip
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("opening a session: {e}"))
}

/// Who this session belongs to, sliding its expiry forward — or `None` for a
/// token that is unknown or expired, **deleting the expired row on the way**.
///
/// Lazily, per §4, with [`sweep_expired_sessions`] as the other half. Neither is
/// enough alone: a server that runs for a month never sweeps at startup, and a
/// row nobody presents is never looked up.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn session_owner(conn: &Connection, token: &str) -> Result<Option<Account>, String> {
    let hashed = credential::token_sha256(token);
    conn.execute(
        "DELETE FROM session WHERE token_sha256 = ?1 AND expires <= datetime('now')",
        params![hashed],
    )
    .map_err(|e| format!("sweeping an expired session: {e}"))?;

    let found = conn
        .query_row(
            "SELECT u.id, u.name, u.role, u.can_download, u.can_stream, u.can_stamp
               FROM session s
               JOIN user u ON u.id = s.user_id
              WHERE s.token_sha256 = ?1",
            params![hashed],
            account_from_row,
        )
        .optional()
        .map_err(|e| format!("resolving a session: {e}"))?;

    if found.is_some() {
        // Best effort, like `by_api_key`'s touch: a failed slide must not fail
        // the request it was recording, and the credential was already valid.
        let _ = conn.execute(
            "UPDATE session
                SET last_seen = datetime('now'),
                    expires   = datetime('now', ?2)
              WHERE token_sha256 = ?1",
            params![hashed, format!("+{SESSION_DAYS} days")],
        );
    }
    Ok(found)
}

/// End one session. Logout.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn close_session(conn: &Connection, token: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM session WHERE token_sha256 = ?1",
        params![credential::token_sha256(token)],
    )
    .map(|_| ())
    .map_err(|e| format!("closing a session: {e}"))
}

/// End every session of this account's except the one named, and answer how
/// many.
///
/// §4: a password change must not leave a compromised session alive — that is
/// the whole point of changing it — and must not log out the browser that made
/// the change.
///
/// **API keys are deliberately untouched.** They are independent credentials
/// with their own revocation, and bundling them would make rotating a password
/// silently break every reader in the house.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn close_other_sessions(conn: &Connection, user_id: i64, keep: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM session WHERE user_id = ?1 AND token_sha256 <> ?2",
        params![user_id, credential::token_sha256(keep)],
    )
    .map_err(|e| format!("ending other sessions: {e}"))
}

/// Delete every rate-limit window that has lapsed.
///
/// **Without this, nothing removes these but a successful login for one exact
/// key.** A loop of `POST /api/session` with a fresh random name each time
/// would leave a row for every name it fails with in a `WITHOUT ROWID` table,
/// for good: an unauthenticated disk-exhaustion path against the one database
/// `serve` and `library import` share. Swept beside the sessions at startup.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn sweep_lapsed_attempts(conn: &Connection) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM login_attempt WHERE window_start <= datetime('now', ?1)",
        params![format!("-{LOCKOUT_WINDOW_MINUTES} minutes")],
    )
    .map_err(|e| format!("sweeping lapsed login attempts: {e}"))
}

/// Delete every expired session. Called once at `serve` startup.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn sweep_expired_sessions(conn: &Connection) -> Result<usize, String> {
    conn.execute("DELETE FROM session WHERE expires <= datetime('now')", [])
        .map_err(|e| format!("sweeping expired sessions: {e}"))
}

/// How long one counting window lasts, and how many failures fit in it.
///
/// §7: a fixed window rather than a sliding one, because the thing being
/// defended is a password guess and the difference between the two shapes is
/// smaller than the difference between "some limit" and "none". Fifteen minutes
/// and ten attempts leaves a person who has genuinely forgotten which password
/// they used room to try, and leaves an attacker forty guesses an hour against
/// an argon2id hash.
pub const LOCKOUT_WINDOW_MINUTES: i64 = 15;
pub const LOCKOUT_AFTER: i64 = 10;

/// The rate-limiter key for a client address and a username.
///
/// **The pair, not the account** (§7). OPDS clients retry hard, and a single
/// misconfigured reader keyed on the account alone would lock its owner out of
/// every device at once — the failure being defended against would be caused by
/// the defence. Keyed on the pair, the misconfigured client locks out only
/// itself.
///
/// A newline joins them because it cannot appear in either half: an address is
/// digits, dots and colons, and a username reaching here came out of an HTTP
/// header. Joining on a character that *could* appear in a name would let two
/// different pairs collide on one key.
#[must_use]
pub fn attempt_key(address: &str, name: &str) -> String {
    format!("{address}\n{name}")
}

/// How many failures one **client address** may accumulate in the same window,
/// whatever names it offered.
///
/// Wider than [`LOCKOUT_AFTER`] on purpose: a household behind one NAT is one
/// address, and several readers each spending their own allowance must not lock
/// each other out. Five accounts' worth, which at 11.9 ms a hash bounds one
/// address to well under a second of argon2 per window.
///
/// **Behind a reverse proxy this counts the proxy unless `--trusted-proxy` is
/// set**, in which case every reader shares one budget. That is the same
/// assumption [`attempt_key`] already makes about its address half rather than a
/// new one, and it is why the flag exists.
pub const ADDRESS_LOCKOUT_AFTER: i64 = 50;

/// The ceiling has to leave room above one account's allowance, or a household
/// behind one NAT locks itself out and the wider key becomes the failure it was
/// added to prevent.
///
/// A compile error rather than a test, for the reason `credential.rs` gives
/// about its argon2 parameters: the thing that would be lowered by mistake is a
/// constant, so the check belongs where the constant is.
const _: () = assert!(
    ADDRESS_LOCKOUT_AFTER > LOCKOUT_AFTER,
    "the address ceiling must leave room above one account's allowance"
);

/// The wider rate-limiter key: the client address alone.
///
/// **[`attempt_key`]'s pair is the right key for lockout and the wrong one for
/// cost.** The name half is supplied by the caller and unbounded, so a fresh
/// invented name is a fresh counter, and [`credential::DECOY_HASH`] makes an
/// unknown name cost what a real one costs: argon2 time and 19 MiB of buffer
/// per request, a denial of service against a counter no attacker need ever
/// approach. Pinned by
/// `session::tests::inventing_a_new_name_every_time_does_not_buy_unlimited_hashing`.
///
/// **Cannot collide with an [`attempt_key`]**: that one always contains a
/// newline and an address never does — an address reaching here is the
/// rendering of an `IpAddr`.
#[must_use]
pub fn address_key(address: &str) -> String {
    address.to_string()
}

/// Record one failed attempt and answer how many are now in the window.
///
/// # Errors
///
/// Returns an error if the upsert fails.
pub fn note_failure(conn: &Connection, key: &str) -> Result<i64, String> {
    // One statement rather than read-then-write: two devices failing at the
    // same moment must not each read 9 and each write 10.
    conn.execute(
        "INSERT INTO login_attempt (key, window_start, count)
         VALUES (?1, datetime('now'), 1)
         ON CONFLICT(key) DO UPDATE SET
           count = CASE
             WHEN login_attempt.window_start <= datetime('now', ?2) THEN 1
             ELSE login_attempt.count + 1
           END,
           window_start = CASE
             WHEN login_attempt.window_start <= datetime('now', ?2)
               THEN datetime('now')
             ELSE login_attempt.window_start
           END",
        params![key, format!("-{LOCKOUT_WINDOW_MINUTES} minutes")],
    )
    .map_err(|e| format!("recording a failed attempt: {e}"))?;
    conn.query_row(
        "SELECT count FROM login_attempt WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .map_err(|e| format!("reading the attempt count: {e}"))
}

/// Whether this pair has spent its attempts inside the current window.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn is_locked_out(conn: &Connection, key: &str) -> Result<bool, String> {
    spent(conn, key, LOCKOUT_AFTER)
}

/// Whether this **address** has spent its wider allowance, whatever names it
/// offered. See [`address_key`] for why the pair alone is not enough.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn is_address_locked_out(conn: &Connection, key: &str) -> Result<bool, String> {
    spent(conn, key, ADDRESS_LOCKOUT_AFTER)
}

/// Whether `key` has reached `ceiling` inside the current window.
///
/// One implementation for both keys, so the window arithmetic cannot drift
/// between them — the two differ only in what they count and how far.
fn spent(conn: &Connection, key: &str, ceiling: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT count FROM login_attempt
          WHERE key = ?1 AND window_start > datetime('now', ?2)",
        params![key, format!("-{LOCKOUT_WINDOW_MINUTES} minutes")],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(|e| format!("reading the attempt count: {e}"))
    .map(|count| count.is_some_and(|count| count >= ceiling))
}

/// Forget this pair's failures. Called on every success, so a person who
/// mistyped twice and then got it right starts clean.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn clear_failures(conn: &Connection, key: &str) -> Result<(), String> {
    conn.execute("DELETE FROM login_attempt WHERE key = ?1", params![key])
        .map(|_| ())
        .map_err(|e| format!("clearing failed attempts: {e}"))
}

/// Give an account a password and a name, **by promoting the row spec A seeded
/// rather than inserting beside it**.
///
/// §5: "Whichever path runs, it must promote that row — same `user.id` — and not
/// insert a new one. Inserting would orphan everything recorded between A and A2
/// on an account nobody can log in as, and the loss would be silent, because the
/// reader would simply show an empty library and a fresh position."
///
/// So this **updates**, and refuses rather than guessing when there is no
/// unclaimed row to update. `add` is the command for a second account, and the
/// two are deliberately not one function that decides for itself: the difference
/// between them is whether a reader's history follows the name, which is not
/// something to infer from whether a row happened to be there.
///
/// # Errors
///
/// Returns [`WriteError::Unusable`] if [`refused_name`] refuses the name, if
/// the password is refused (it is empty), or if another account holds the
/// name; [`WriteError::Refused`] if no unclaimed account
/// exists — every account already has a password, so there is nothing to
/// promote and `add` is the command wanted — or if the role would leave the
/// library with no administrator; [`WriteError::Failed`] if a read or the
/// update fails.
/// **The caller holds the connection across `hash_password` here**, unlike
/// `auth::from_basic`, and that is a deliberate difference rather than the same
/// oversight: these are CLI and setup paths that run once with nobody else
/// waiting, where `from_basic` is on every request. The rule the request path
/// follows — never hold the library's one mutex across argon2 — is stated at its
/// own call site so that neither reads as the other's precedent. Noted by the
/// second whole-branch review.
pub fn promote(
    conn: &Connection,
    name: &str,
    password: &str,
    role: &str,
) -> Result<i64, WriteError> {
    if let Some(why) = refused_name(name) {
        return Err(WriteError::Unusable(why.to_string()));
    }
    let id = unclaimed_row(conn)
        .map_err(WriteError::Failed)?
        .ok_or_else(|| WriteError::Refused(NOTHING_TO_PROMOTE.to_string()))?;
    // **Refuse to leave the library with nobody who can administer it.**
    // Promoting the only unclaimed row to a role that cannot administer would
    // leave nothing for `unclaimed` to find a second time, and the claim route,
    // which reads `is_claimed`, would be shut. Adding an admin first and
    // promoting to `user` afterwards still works — the rule is the invariant,
    // not the order.
    if role != "admin" && !admin_exists(conn).map_err(WriteError::Failed)? {
        return Err(WriteError::Refused(format!(
            "promoting the only account to `{role}` would leave this library with no \
             administrator and no way to add one, since there would be nothing \
             left to promote. Run it without `--role`, or `taiyaku library user \
             add <name> --role admin` first"
        )));
    }
    let phc = credential::hash_password(password).map_err(WriteError::Unusable)?;
    conn.execute(
        "UPDATE user SET name = ?2, password_hash = ?3, role = ?4 WHERE id = ?1",
        params![id, name, phc, role],
    )
    .map_err(|e| {
        if is_unique_violation(&e) {
            WriteError::Unusable(taken(name))
        } else {
            WriteError::Failed(format!("promoting account {id} to `{name}`: {e}"))
        }
    })?;
    Ok(id)
}

/// Why no account is given an empty name. An account's page is at its name
/// (`web/src/configAddress.ts`), and an empty one has no address: what
/// `/config/users/` reaches is the table.
const NO_NAME: &str = "an account needs a name, since its page is at its name";

/// Why no account can be called `name`, whatever other accounts hold: it is
/// empty, or [`crate::has_no_address`] answers true for it. `None` for any
/// other name. Every write of a name a caller gave to `user.name` asks this
/// first.
fn refused_name(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some(NO_NAME)
    } else if crate::has_no_address(name) {
        Some(crate::NO_ADDRESS)
    } else {
        None
    }
}

/// `Ok` where [`add`] or [`promote`] would give an account `name`, or the
/// refusal either would answer: [`refused_name`]'s, or that another account
/// holds it. `keeping` is the account that would take the name, whose own
/// name is no clash: the one [`promote`] reaches; `None` for [`add`].
///
/// **For a caller that asks something of the person before it writes**, as
/// the CLI asks for a password twice: refused only by the write, a name it
/// was never going to take costs the person the secret typed for nothing.
/// The writers still refuse for themselves, so a name taken between this
/// read and the write is refused there.
///
/// # Errors
///
/// Returns [`WriteError::Unusable`] for a name either writer refuses, and
/// [`WriteError::Failed`] if the read fails.
pub fn check_name(conn: &Connection, name: &str, keeping: Option<i64>) -> Result<(), WriteError> {
    if let Some(why) = refused_name(name) {
        return Err(WriteError::Unusable(why.to_string()));
    }
    match id_of(conn, name).map_err(WriteError::Failed)? {
        Some(holder) if Some(holder) != keeping => Err(WriteError::Unusable(taken(name))),
        _ => Ok(()),
    }
}

/// Why no account can be called `name`: another holds it. Names are stored as
/// written and compared byte for byte, so the holder's name is `name`. The
/// words are `libraries::rename`'s for a taken library name, and the
/// Configuration pages say the same sentence as a name is typed
/// (`accountNameClash`, `web/src/configAddress.ts`), so the two change
/// together.
fn taken(name: &str) -> String {
    format!("an account is already called `{name}`")
}

/// Whether `e` is a `UNIQUE` constraint refusing a write. `user.name` is the
/// one `UNIQUE` column a write to `user` can collide on (the id is a primary
/// key, refused under a code of its own), so a write to `user` refused so was
/// refused a name another account holds.
fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

/// The account a [`promote`] would reach, or the refusal explaining that there
/// is none.
///
/// **Split out so a caller can ask before it prompts.** A refusal that comes
/// after `taiyaku library user promote` has read a password twice is the worst
/// order: the person has done the work and typed the secret before being told
/// the command was never going to do anything. The rule lives here so both
/// callers share it rather than the CLI carrying a second copy that can drift.
///
/// # Errors
///
/// Returns an error if every account already has a password — naming `user add`
/// as the command that does what was wanted — or if the query fails.
pub fn unclaimed(conn: &Connection) -> Result<i64, String> {
    unclaimed_row(conn)?.ok_or_else(|| NOTHING_TO_PROMOTE.to_string())
}

/// [`unclaimed`]'s refusal when every account has a password.
const NOTHING_TO_PROMOTE: &str = "every account in this library already has a password, so \
     there is none to promote. `taiyaku library user add <name>` creates another";

/// The account a [`promote`] would reach, if any. Apart from [`unclaimed`] so
/// that [`promote`] can tell "none" (refused) from a failed read.
fn unclaimed_row(conn: &Connection) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM user WHERE password_hash = ?1 ORDER BY id LIMIT 1",
        params![credential::LOCKED],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("looking for an unclaimed account: {e}"))
}

/// Create a second account.
///
/// # Errors
///
/// Returns an error if [`refused_name`] refuses the name, if the name is
/// taken, or if the insert fails.
pub fn add(conn: &Connection, name: &str, password: &str, role: &str) -> Result<i64, String> {
    if let Some(why) = refused_name(name) {
        return Err(why.to_string());
    }
    let phc = credential::hash_password(password)?;
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES (?1, ?2, ?3)",
        params![name, phc, role],
    )
    .map_err(|e| {
        if is_unique_violation(&e) {
            taken(name)
        } else {
            format!("adding the account `{name}`: {e}")
        }
    })?;
    Ok(conn.last_insert_rowid())
}

/// The account with this id, or `None`.
///
/// [`by_name`] answers the login question and returns the stored hash beside
/// the account; this one answers "which row is id 42" for an administrative
/// screen that already has the id and must never see a hash.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn by_id(conn: &Connection, id: i64) -> Result<Option<Account>, String> {
    conn.query_row(
        "SELECT id, name, role, can_download, can_stream, can_stamp FROM user WHERE id = ?1",
        params![id],
        account_from_row,
    )
    .optional()
    .map_err(|e| format!("looking up account {id}: {e}"))
}

/// Which libraries an account sees, as [`update`] sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// Every library, including any added later. The account's grants are
    /// deleted.
    Every,
    /// Only these libraries, by id, replacing the account's grants. **An
    /// empty list is no library**, never every library.
    Libraries(Vec<i64>),
}

/// What [`update`] may change about one account.
///
/// **Every field is "leave it alone" when `None`**, including `grant`: `None`
/// does not touch the grants, where `Some(Grant::Libraries(vec![]))` leaves
/// the account seeing no library.
///
/// **`password` is an administrator resetting somebody else's**, and it is
/// here rather than left to [`set_password`] because the two have to succeed
/// or fail together. [`update`] commits its own scope and `set_password` opens
/// none, so a store fault on a password set separately would leave the role
/// and capability changes **committed** beside a failed request. A
/// half-applied `PATCH`, on the one route that edits permissions.
///
/// The hash is taken **before** [`update`]'s scope opens, so `None` costs
/// nothing and `Some` costs one argon2 outside the transaction rather than
/// inside it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patch {
    pub name: Option<String>,
    pub role: Option<String>,
    pub can_download: Option<bool>,
    pub can_stream: Option<bool>,
    pub can_stamp: Option<bool>,
    /// What the account sees, replacing what it saw; `None` leaves it alone.
    pub grant: Option<Grant>,
    pub password: Option<String>,
}

/// Why one of this module's writers would not do what it was asked.
///
/// **Three cases rather than one `String`**, because the caller has to tell
/// them apart.
///
/// **Shared by [`update`], [`remove`], [`removal_cost`] and [`promote`] rather
/// than one type per function**, so that [`removal_cost`]'s quote, the safety
/// feature on the destructive route, refuses as [`remove`] does and does not
/// report a routine refusal as a server fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The caller's own mistake: an empty password, a name already taken or
    /// one [`refused_name`] refuses, a role the schema's
    /// `CHECK (role IN ('admin', 'user'))` refuses.
    Unusable(String),
    /// Well-formed, names a real account, and still not allowed — spec C4
    /// §4's refusals.
    Refused(String),
    /// A store fault. Nothing about the request caused it.
    Failed(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unusable(why) | Self::Refused(why) | Self::Failed(why) => f.write_str(why),
        }
    }
}

/// Whether some account **other than** `excluding` is an administrator who can
/// actually log in.
///
/// **The same "role is not enough" reading [`admin_exists`] takes**, and for
/// the same reason: migration 1 seeds an admin row that migration 4 locks, so
/// counting roles alone would let the last real administrator be demoted or
/// deleted behind a row nobody can log in as.
fn another_admin_can_log_in(conn: &Connection, excluding: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM user
          WHERE role = 'admin' AND password_hash <> ?1 AND id <> ?2",
        params![credential::LOCKED, excluding],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .map_err(|e| format!("counting the other administrators: {e}"))
}

/// Change an account, refusing the two changes spec C4 §4 says would lock the
/// library's administrator out of it.
///
/// `acting_as` is the account making the change, or `None` for a caller with
/// no session to be — which is what the CLI is. It exists for exactly one
/// rule, C4 §4's second refusal, and that rule cannot be enforced without it.
///
/// **The two role refusals are not one rule, and merging them ships neither.**
/// The first is about the *library's* state after the change and fires for any
/// caller; the second is about *who* is asking and fires even when the library
/// keeps an administrator. A single "is this the last admin" check satisfies
/// the letter of both and lets an administrator demote themselves whenever a
/// second one exists, which C4 §4 forbids in as many words.
///
/// **`role` is validated by the schema, not here.** `004_auth.sql` gives the
/// column `CHECK (role IN ('admin', 'user'))`; a second list of the two words
/// in this function is a second list to drift.
///
/// # Errors
///
/// Returns an error if the patch names the account with a name
/// [`refused_name`] refuses, if `id` names no account, if either
/// refusal fires, or if SQLite refuses the write — which includes a `role`
/// the schema's `CHECK` rejects.
pub fn update(
    conn: &Connection,
    id: i64,
    patch: &Patch,
    acting_as: Option<i64>,
) -> Result<Account, WriteError> {
    if let Some(why) = patch.name.as_deref().and_then(refused_name) {
        return Err(WriteError::Unusable(why.to_string()));
    }
    // **The hash is taken before the scope opens**, so an empty password is
    // refused before anything is written and no argon2 runs inside a
    // transaction. [`Patch`]'s own doc carries the argument.
    let phc = match patch.password.as_deref() {
        Some(password) => Some(credential::hash_password(password).map_err(WriteError::Unusable)?),
        None => None,
    };

    // **The scope opens before the invariant is read, not after.** This
    // connection is configured `TransactionBehavior::Immediate` (`lib.rs`), so
    // `Nested::open` takes the write lock at `BEGIN` — which means the
    // "is there another administrator" question and the demotion that acts on
    // its answer are one atomic step. Read outside, they would be two: the CLI
    // is a documented second writer in a second process, and it demoting one
    // administrator while this demotes another could leave the library with
    // none — the single state this check exists to prevent.
    let scope = crate::Nested::open(conn, "account_update")
        .map_err(|e| WriteError::Failed(format!("updating account {id}: {e}")))?;

    let current = by_id(conn, id)
        .map_err(WriteError::Failed)?
        .ok_or_else(|| WriteError::Unusable(format!("no account with id {id}")))?;

    if let Some(role) = patch.role.as_deref()
        && role != current.role
        && current.role == "admin"
    {
        if acting_as == Some(id) {
            return Err(WriteError::Refused(
                "an administrator cannot remove their own admin role; another \
                 administrator has to do it"
                    .to_string(),
            ));
        }
        if !another_admin_can_log_in(conn, id).map_err(WriteError::Failed)? {
            return Err(WriteError::Refused(format!(
                "demoting `{}` would leave this library with no administrator who can \
                 log in. Give another account the admin role first",
                current.name
            )));
        }
    }

    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(v) = &patch.name {
        values.push(Box::new(v.clone()));
        sets.push(format!("name = ?{}", values.len()));
    }
    if let Some(v) = &patch.role {
        values.push(Box::new(v.clone()));
        sets.push(format!("role = ?{}", values.len()));
    }
    if let Some(v) = patch.can_download {
        values.push(Box::new(i64::from(v)));
        sets.push(format!("can_download = ?{}", values.len()));
    }
    if let Some(v) = patch.can_stream {
        values.push(Box::new(i64::from(v)));
        sets.push(format!("can_stream = ?{}", values.len()));
    }
    if let Some(v) = patch.can_stamp {
        values.push(Box::new(i64::from(v)));
        sets.push(format!("can_stamp = ?{}", values.len()));
    }

    if !sets.is_empty() {
        values.push(Box::new(id));
        let sql = format!(
            "UPDATE user SET {} WHERE id = ?{}",
            sets.join(", "),
            values.len()
        );
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(AsRef::as_ref).collect();
        conn.execute(&sql, refs.as_slice()).map_err(|e| {
            // **The message names the value SQLite refused; SQLite's own does
            // not.** Two of the four columns carry a constraint — `role` a
            // `CHECK (role IN ('admin', 'user'))` and `name` a `UNIQUE` — and
            // both report the constraint rather than the input, so a person
            // reading the refusal sees "CHECK constraint failed: role IN
            // ('admin', 'user')" with nothing saying what they typed. Named
            // by echoing the patch rather than by re-listing the two allowed
            // words here, which would be a second list to drift from the
            // schema's. A taken name is said in [`taken`]'s words, the ones
            // the pages draw as a name is typed.
            if let (true, Some(name)) = (is_unique_violation(&e), &patch.name) {
                return WriteError::Unusable(taken(name));
            }
            let mut named = format!("updating account {id}");
            if let Some(role) = &patch.role {
                named.push_str(&format!(", role `{role}`"));
            }
            if let Some(name) = &patch.name {
                named.push_str(&format!(", name `{name}`"));
            }
            // **`Unusable`, not `Failed`.** Everything this statement can
            // refuse is the caller's own input — the `CHECK` on `role`, the
            // `UNIQUE` on `name` — so a 500 here would report a typo as a
            // server fault. A store fault mid-statement is misfiled as a 400
            // by the same token, and that is the trade: the two constraint
            // violations are the overwhelmingly likely cause and the only
            // ones a caller can act on.
            WriteError::Unusable(format!("{named}: {e}"))
        })?;
    }

    // **Replace-all, diffed rather than deleted and reinserted.** Deleting a
    // row and reinserting it under a fresh id nulls every reference to the old
    // one through an `ON DELETE SET NULL`. Nothing refers to a `user_library`
    // row that way, but a diff is the same size of code and cannot acquire
    // that class of bug later.
    //
    // **An empty list sets "only these" and names none**, so the account sees
    // no library: emptying a grant must never widen it to every library.
    match &patch.grant {
        None => {}
        Some(Grant::Every) => {
            set_every_library(conn, id)
                .map_err(|e| WriteError::Failed(format!("updating account {id}: {e}")))?;
        }
        Some(Grant::Libraries(wanted)) => {
            let mut wanted_libraries = std::collections::BTreeSet::new();
            for library_id in wanted {
                // **An id no library holds is the caller's mistake, not a
                // fault.** Left to `grant_library`, it reaches the client as a
                // 500. The list is checked whole before a grant is written, and
                // a refusal rolls back whatever the same patch wrote before it,
                // since the scope is never committed.
                if !library_exists(conn, *library_id).map_err(WriteError::Failed)? {
                    return Err(WriteError::Unusable(format!(
                        "there is no library {library_id} to grant"
                    )));
                }
                wanted_libraries.insert(*library_id);
            }
            conn.execute(
                "UPDATE user SET every_library = 0 WHERE id = ?1",
                params![id],
            )
            .map_err(|e| WriteError::Failed(format!("updating account {id}: {e}")))?;
            let held: std::collections::BTreeSet<i64> = libraries_for(conn, id)
                .map_err(WriteError::Failed)?
                .into_iter()
                .collect();
            for library in held.difference(&wanted_libraries) {
                revoke_library(conn, id, *library).map_err(WriteError::Failed)?;
            }
            for library in wanted_libraries.difference(&held) {
                grant_library(conn, id, *library).map_err(WriteError::Failed)?;
            }
        }
    }

    // **The password lands inside this scope, with the columns above it.**
    // Written here rather than through [`set_password`] for the reason
    // [`Patch`]'s own doc gives: that function opens no transaction of its
    // own, so calling it *after* this one committed would leave a store fault
    // able to produce a half-applied patch.
    //
    // **Every session of the account's ends**, which is A2 §4's rule for an
    // administrator resetting somebody else's password: there is no session
    // of the *target's* that made the change, so there is none to keep.
    // API keys are deliberately untouched — independent credentials with
    // their own revocation.
    if let Some(phc) = phc {
        conn.execute(
            "UPDATE user SET password_hash = ?2 WHERE id = ?1",
            params![id, phc],
        )
        .map_err(|e| WriteError::Failed(format!("setting the password for account {id}: {e}")))?;
        conn.execute("DELETE FROM session WHERE user_id = ?1", params![id])
            .map_err(|e| WriteError::Failed(format!("ending sessions for account {id}: {e}")))?;
    }

    scope
        .commit()
        .map_err(|e| WriteError::Failed(format!("committing the update of account {id}: {e}")))?;

    by_id(conn, id)
        .map_err(WriteError::Failed)?
        .ok_or_else(|| WriteError::Failed(format!("account {id} vanished mid-update")))
}

/// Replace an account's password, **ending every session it has open**.
///
/// §4: a compromised session must not outlive the password change that was meant
/// to end it. `keep` is the session making the change, so a reader changing their
/// own password from a browser is not logged out of it; `None` ends them all,
/// which is what an admin resetting somebody else's password means.
///
/// **API keys are deliberately untouched** — independent credentials with their
/// own revocation, and bundling them would make rotating a password silently
/// break every reader in the house.
///
/// # Errors
///
/// Returns an error if the account does not exist, or if either write fails.
pub fn set_password(
    conn: &Connection,
    user_id: i64,
    password: &str,
    keep: Option<&str>,
) -> Result<usize, String> {
    let phc = credential::hash_password(password)?;
    let changed = conn
        .execute(
            "UPDATE user SET password_hash = ?2 WHERE id = ?1",
            params![user_id, phc],
        )
        .map_err(|e| format!("setting the password for account {user_id}: {e}"))?;
    if changed == 0 {
        return Err(format!("no account with id {user_id}"));
    }
    match keep {
        Some(token) => close_other_sessions(conn, user_id, token),
        None => conn
            .execute("DELETE FROM session WHERE user_id = ?1", params![user_id])
            .map_err(|e| format!("ending sessions for account {user_id}: {e}")),
    }
}

/// Mint an API key for an account and store only its hash.
///
/// **Returns the secret once.** The row holds `secret_sha256` and nothing else,
/// so there is nothing to show a second time even by mistake — which is the
/// property, not a limitation of this function.
///
/// # Errors
///
/// Returns an error if the account does not exist or the insert fails.
/// **`api_key.expires` is honoured by [`by_api_key`] and written by nothing**,
/// which is recorded rather than left to be discovered: there is no CLI flag and
/// no route that sets one, so the expiry branch is unreachable outside a test.
/// It stays in the schema because it is cheap and a key that should expire is an
/// ordinary want; what it must not become is a column somebody assumes is being
/// maintained. Spec C4's configuration UI is where a flag for it belongs. Noted
/// by the second whole-branch review.
pub fn create_key(conn: &Connection, user_id: i64, name: &str) -> Result<String, String> {
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (?1, ?2, ?3)",
        params![user_id, name, credential::token_sha256(&secret)],
    )
    .map_err(|e| format!("creating an API key for account {user_id}: {e}"))?;
    Ok(secret)
}

/// Revoke one API key **of this account**, answering whether there was one.
///
/// Scoped to `user_id` in the statement rather than by a caller checking first,
/// exactly as [`revoke_session`] is and for the same reason: an id from one
/// account's listing must not delete another account's key, and a function that
/// took the id alone would depend on every future caller remembering to check.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn revoke_key(conn: &Connection, user_id: i64, key_id: i64) -> Result<bool, String> {
    conn.execute(
        "DELETE FROM api_key WHERE user_id = ?1 AND id = ?2",
        params![user_id, key_id],
    )
    .map(|rows| rows > 0)
    .map_err(|e| format!("revoking API key {key_id}: {e}"))
}

/// Every account, ascending by id, with whether it can log in.
///
/// The second half is the one worth printing: an account still holding
/// [`credential::LOCKED`] has a row and no way in, and nothing else about it
/// says so.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn list(conn: &Connection) -> Result<Vec<(Account, bool)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, role, can_download, can_stream, can_stamp, password_hash
               FROM user ORDER BY id",
        )
        .map_err(|e| format!("preparing the account list: {e}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                account_from_row(row)?,
                row.get::<_, String>("password_hash")? != credential::LOCKED,
            ))
        })
        .map_err(|e| format!("listing accounts: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("listing accounts: {e}"))
}

/// One API key as an account screen needs to show it — never its secret, which
/// is not stored and so cannot be shown.
///
/// `last_used` is `None` for a key nobody has presented yet, and that is the
/// column a person reads when deciding which of three keys is the tablet they
/// lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySummary {
    pub id: i64,
    pub name: String,
    pub created: String,
    pub last_used: Option<String>,
}

/// One account's API keys, ascending by id.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn keys_of(conn: &Connection, user_id: i64) -> Result<Vec<KeySummary>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, created, last_used FROM api_key
              WHERE user_id = ?1 ORDER BY id",
        )
        .map_err(|e| format!("preparing the key list: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| {
            Ok(KeySummary {
                id: row.get("id")?,
                name: row.get("name")?,
                created: row.get("created")?,
                last_used: row.get("last_used")?,
            })
        })
        .map_err(|e| format!("listing API keys: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("listing API keys: {e}"))
}

/// An account's id, by name.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn id_of(conn: &Connection, name: &str) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM user WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up the account `{name}`: {e}"))
}

/// One open session as an account screen lists it.
///
/// **`id` is the token's sha256**, which is also its primary key — the schema
/// carries no second identifier precisely so there is no column a raw token
/// could land in. Handing the hash to a browser gives away nothing: it is what
/// the server compares against, not what a client presents, and inverting it is
/// the sha256 preimage problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub created: String,
    pub last_seen: String,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    /// Whether this is the session doing the asking, so a reader can tell which
    /// row is the browser in front of them before revoking one.
    pub current: bool,
}

/// One account's open sessions, newest first.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn sessions_of(
    conn: &Connection,
    user_id: i64,
    current_token: Option<&str>,
) -> Result<Vec<SessionSummary>, String> {
    let current = current_token.map(credential::token_sha256);
    let mut statement = conn
        .prepare(
            "SELECT token_sha256, created, last_seen, user_agent, ip
               FROM session WHERE user_id = ?1
              ORDER BY last_seen DESC",
        )
        .map_err(|e| format!("preparing the session list: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| {
            let id: String = row.get("token_sha256")?;
            Ok(SessionSummary {
                current: current.as_deref() == Some(id.as_str()),
                id,
                created: row.get("created")?,
                last_seen: row.get("last_seen")?,
                user_agent: row.get("user_agent")?,
                ip: row.get("ip")?,
            })
        })
        .map_err(|e| format!("listing sessions: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("listing sessions: {e}"))
}

/// Revoke one session **of this account**, by the id [`sessions_of`] gave.
///
/// Scoped to `user_id` rather than taking the hash alone: an id from one
/// account's listing must not end another account's session, and a route that
/// took only the hash would depend on nobody ever guessing one.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn revoke_session(conn: &Connection, user_id: i64, id: &str) -> Result<bool, String> {
    conn.execute(
        "DELETE FROM session WHERE user_id = ?1 AND token_sha256 = ?2",
        params![user_id, id],
    )
    .map(|rows| rows > 0)
    .map_err(|e| format!("revoking a session: {e}"))
}

// --- removal -------------------------------------------------------------

/// What deleting one account costs.
///
/// **Named after [`crate::scan::Removal`] field for field where the two
/// overlap** — `positions` and `notes` mean the same words on both — because
/// root removal and account removal are the same shape (count, then act,
/// through one route that answers the identical body for a quote and a
/// commit), and two destructive paths that look alike should read alike.
///
/// **No work, edition or member appears here, and that is the difference from
/// a root's removal.** Deleting a person deletes none of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub name: String,
    pub positions: usize,
    pub notes: usize,
    pub sessions: usize,
    pub keys: usize,
    pub grants: usize,
}

/// The five counts and the name, as one row.
///
/// **One statement rather than five round trips**, and shared by
/// [`removal_cost`] and [`remove`] so the quote and the commit can never count
/// differently.
fn count_for(conn: &Connection, id: i64, name: String) -> Result<Removal, String> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM progress     WHERE user_id = ?1),
                (SELECT COUNT(*) FROM note         WHERE user_id = ?1),
                (SELECT COUNT(*) FROM session      WHERE user_id = ?1),
                (SELECT COUNT(*) FROM api_key      WHERE user_id = ?1),
                (SELECT COUNT(*) FROM user_library WHERE user_id = ?1)",
        params![id],
        |row| {
            Ok(Removal {
                name,
                positions: row.get::<_, i64>(0)?.unsigned_abs() as usize,
                notes: row.get::<_, i64>(1)?.unsigned_abs() as usize,
                sessions: row.get::<_, i64>(2)?.unsigned_abs() as usize,
                keys: row.get::<_, i64>(3)?.unsigned_abs() as usize,
                grants: row.get::<_, i64>(4)?.unsigned_abs() as usize,
            })
        },
    )
    .map_err(|e| format!("counting what removing account {id} costs: {e}"))
}

/// Count what [`remove`] would take, without taking any of it.
///
/// **Runs [`remove`]'s refusal too, and that is the point of the quote.** A dry
/// run whose whole purpose is to state what the commit will do must not answer
/// `200 OK` with five counts for a deletion the commit will then refuse — an
/// operator would read the counts, confirm, and meet the refusal afterwards.
///
/// # Errors
///
/// [`WriteError::Unusable`] for an `id` naming no account,
/// [`WriteError::Refused`] if removing it would leave the library with no
/// administrator who can log in, [`WriteError::Failed`] if a count fails.
pub fn removal_cost(conn: &Connection, id: i64) -> Result<Removal, WriteError> {
    let account = by_id(conn, id)
        .map_err(WriteError::Failed)?
        .ok_or_else(|| WriteError::Unusable(format!("no account with id {id}")))?;
    refuse_removing_the_last_admin(conn, &account)?;
    count_for(conn, id, account.name).map_err(WriteError::Failed)
}

/// The one statement of C4 §4's first refusal, read by [`remove`] and by
/// [`removal_cost`] so the quote and the commit cannot disagree.
fn refuse_removing_the_last_admin(conn: &Connection, account: &Account) -> Result<(), WriteError> {
    if account.role == "admin"
        && !another_admin_can_log_in(conn, account.id).map_err(WriteError::Failed)?
    {
        return Err(WriteError::Refused(format!(
            "removing `{}` would leave this library with no administrator who can \
             log in. Give another account the admin role first",
            account.name
        )));
    }
    Ok(())
}

/// Delete an account and everything `ON DELETE CASCADE` takes with it,
/// answering the counts from **before** the delete.
///
/// **The counts are taken inside the transaction that deletes**, exactly as
/// `scan::apply_removal` takes a root's: counting afterwards answers zero for
/// the numbers this function exists to state honestly.
///
/// **Refuses to delete the last administrator who can log in**, which is C4
/// §4's first refusal and the same invariant [`update`] holds against a
/// demotion. [`promote`] states the third face of it.
///
/// # Errors
///
/// [`WriteError::Unusable`] for an `id` naming no account,
/// [`WriteError::Refused`] if the last-administrator rule fires,
/// [`WriteError::Failed`] if SQLite refuses the delete.
pub fn remove(conn: &Connection, id: i64) -> Result<Removal, WriteError> {
    // **The scope opens before the invariant is read**, for the reason
    // [`update`] states: this connection is `TransactionBehavior::Immediate`,
    // so the write lock is taken at `BEGIN` and the check cannot be overtaken
    // by the CLI in another process between reading and acting.
    let scope = crate::Nested::open(conn, "account_remove")
        .map_err(|e| WriteError::Failed(format!("removing account {id}: {e}")))?;

    let account = by_id(conn, id)
        .map_err(WriteError::Failed)?
        .ok_or_else(|| WriteError::Unusable(format!("no account with id {id}")))?;
    refuse_removing_the_last_admin(conn, &account)?;

    let cost = count_for(conn, id, account.name).map_err(WriteError::Failed)?;
    conn.execute("DELETE FROM user WHERE id = ?1", params![id])
        .map_err(|e| WriteError::Failed(format!("removing account {id}: {e}")))?;
    scope
        .commit()
        .map_err(|e| WriteError::Failed(format!("committing the removal of account {id}: {e}")))?;
    Ok(cost)
}

#[cfg(test)]
mod tests;

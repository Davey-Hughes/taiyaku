-- Spec A2's schema: accounts that can log in, the two token kinds, per-root
-- sharing, and the rate limiter's window.
-- See docs/superpowers/specs/2026-08-14-library-auth-design.md §3.
--
-- **This migration rebuilds `user`, and that is why `migrate_with` turns
-- foreign keys off around the whole run rather than this file turning them off
-- inside itself.** `PRAGMA foreign_keys` is a no-op within a transaction — it
-- does not error and does not warn, it simply does not apply — and
-- `apply_pending` runs every migration inside one. `DROP TABLE` on a parent
-- performs an implicit DELETE that fires `ON DELETE CASCADE`, and `progress`
-- and `note` are the two tables referencing `user(id)`. They hold every note
-- and every reading position in the library. Written the obvious way, inside
-- the transaction, this migration copies the `user` rows correctly and deletes
-- all of them. Measured 2026-08-18: notes surviving, 0. See `docs/notes.md`,
-- and `schema::tests::migration_4_rebuilds_user_without_taking_the_notes_with_it`,
-- which fails the moment that wrapper is removed.

ALTER TABLE user ADD COLUMN can_download INTEGER NOT NULL DEFAULT 1;
ALTER TABLE user ADD COLUMN can_stream   INTEGER NOT NULL DEFAULT 1;

-- `password_hash` NULL -> NOT NULL, the documented twelve-step rebuild.
-- `001_initial.sql`'s own comment asks for it: "`password_hash` is nullable for
-- exactly the window between this spec and A2 ... A2 MUST make it NOT NULL".
--
-- Migration 1 seeds `(1, 'local', 'admin')` with a NULL hash, so "make it NOT
-- NULL" needs an answer for that row rather than a constraint it violates. It
-- gets `'!'`: not a well-formed PHC string, so `credential::verify_password`
-- refuses it for every password, the way /etc/shadow refuses a locked account.
-- `account::admin_exists` reads the same placeholder, which is why a fresh
-- library has an admin row and no admin — and therefore why §5's claim route
-- is still open on it. `taiyaku library user promote` replaces it, and §5
-- requires that promotion reach THIS row, id 1, because every note and reading
-- position recorded between spec A and now hangs off it.
--
-- No index, trigger or view names `user`, so there is nothing to recreate after
-- the rename — checked against `001_initial.sql`, whose three indexes are on
-- `member`, `note` and `work`, and whose six triggers are the two FTS pairs.
CREATE TABLE user_new (
  id            INTEGER PRIMARY KEY,
  name          TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  -- **Defaults to `user` and refuses anything but the two words.** Migration 1
  -- wrote `DEFAULT 'admin'` when there was one account and no login, and this
  -- rebuild carried it forward unexamined — so any insert that omitted the
  -- column minted an administrator, which is the same fail-open the CLI's
  -- `role_of` had. There was no `CHECK` either, so a typo stored a role that is
  -- neither and `Principal::from_account`'s `== "admin"` quietly read it as a
  -- reader.
  --
  -- Amended here rather than in a later migration for the reason 005 states:
  -- 004 has never been merged, so no database has applied it and there is
  -- nothing for an edit to diverge from. Found by the second whole-branch
  -- review.
  role          TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'user')),
  created       TEXT NOT NULL DEFAULT (datetime('now')),
  can_download  INTEGER NOT NULL DEFAULT 1,
  can_stream    INTEGER NOT NULL DEFAULT 1
);

INSERT INTO user_new (id, name, password_hash, role, created, can_download, can_stream)
SELECT id, name, COALESCE(password_hash, '!'), role, created, can_download, can_stream
FROM user;

DROP TABLE user;
ALTER TABLE user_new RENAME TO user;

-- Keyed by the token's HASH rather than carrying a separate id, so there is no
-- second identifier and no column a raw token could be stored in by accident:
-- the column that would hold it does not exist (§2).
--
-- `expires` is slid forward on use rather than fixed at creation, so a browser
-- read once a month never logs out and one abandoned for thirty days does.
CREATE TABLE session (
  token_sha256 TEXT PRIMARY KEY,
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  created      TEXT NOT NULL DEFAULT (datetime('now')),
  last_seen    TEXT NOT NULL DEFAULT (datetime('now')),
  expires      TEXT NOT NULL,
  user_agent   TEXT,
  ip           TEXT
) WITHOUT ROWID;

CREATE INDEX session_by_user ON session (user_id);

-- `name` is what a person calls the device, so revoking the right one does not
-- require guessing which opaque id is the tablet.
--
-- **sha256 rather than argon2, and that is a decision rather than an
-- oversight** (§2): every page request from an OPDS client carries this
-- credential, so a deliberately slow KDF at ~100 ms would put roughly 40
-- seconds of pure hashing on a 400-page volume. There is nothing for slowness
-- to buy against 256 bits from a CSPRNG.
--
-- `expires` NULL means it does not.
CREATE TABLE api_key (
  id            INTEGER PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  secret_sha256 TEXT NOT NULL UNIQUE,
  created       TEXT NOT NULL DEFAULT (datetime('now')),
  last_used     TEXT,
  expires       TEXT
);

CREATE INDEX api_key_by_user ON api_key (user_id);

-- Per-root sharing.
--
-- **Nothing filters on this yet, and that is recorded rather than
-- discovered.** Spec A2 §8 expected spec B to land afterwards and give `work` a
-- `root_id`; B landed first and added neither that column nor any other
-- work-to-root link — `scan_entry.root_id` and `rule.root_id` exist, `member`
-- has no `scan_entry_id`. Meanwhile `serve` is handed sidecar paths and no
-- route lists by root, so there is nothing for a filter to filter. The guard
-- resolves `Principal.roots` from here today; spec C1's browse API is the first
-- caller that can act on the answer.
--
-- In migration 4 rather than deferred to C1 because it holds hand-entered data,
-- which is the store design's §10 rule for what goes in early.
CREATE TABLE user_root (
  user_id INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  root_id INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  PRIMARY KEY (user_id, root_id)
) WITHOUT ROWID;

-- `key` is the (client address, username) pair joined, per §7 — NOT the account
-- alone. OPDS clients retry hard, and keying on the account would let one
-- misconfigured reader lock its owner out of every device at once. Keyed on the
-- pair, the misconfigured client locks out only itself.
CREATE TABLE login_attempt (
  key          TEXT PRIMARY KEY,
  window_start TEXT NOT NULL,
  count        INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

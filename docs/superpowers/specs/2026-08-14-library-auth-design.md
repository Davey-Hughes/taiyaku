# The library's authentication — design

**Status: settled in brainstorming on 2026-08-14. Built on 2026-08-18**, on the
`library-auth` branch; `docs/superpowers/plans/2026-08-18-library-auth.md` is the
plan and `docs/status.md` is what shipped.

**Six statements below were true when written and were made stale by specs A and
B landing first.** They are corrected in place and marked, rather than rewritten
away, because two of them were wrong in ways worth reading: §8 predicted an
ordering that reversed, and §3's migration number moved twice. Each correction is
dated and signed *(corrected 2026-08-18)*.

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **spec A2** of the roadmap in
`2026-08-14-library-store-design.md`, §10. It depends on A and lands **before
C**, so the browse UI is built behind a guard rather than retrofitted into one.

## Where it came from

> I am planning on hosting this as a replacement for my komga and stump hosts,
> so I want full auth capability

That sentence changes the threat model rather than adding a feature. Everything
before it assumed a server on `127.0.0.1` read by one person. This assumes a
server behind a public reverse proxy, read by several people, some of whom
should not see every root.

## 1. What is already right, and what is not there at all

**Nothing authenticates today.** Every route in `build_router` is open.

Two things are already correct and should not be re-litigated:

- **The request log will not leak credentials.** `LOGGED_HEADERS` is a
  five-entry allowlist and its doc comment names the reason: everything *"would
  put a `Cookie` or an `Authorization` into a file that gets read by scripts and
  pasted into issues"* (`serve/reqlog.rs`; the line was 24 and is now 44 —
  *corrected 2026-08-18*, and cited by symbol from here on for the reason the
  open-items table gives about line numbers). Adding auth does not open a hole
  there, and the allowlist must stay an allowlist.
- **Outbound auth already exists as a precedent**, not as a component to reuse:
  the Stump client sends `Authorization: Bearer` (`serve/stump/http.rs:82`).
  That is the opposite direction and shares no code with this.

One thing needs care and is recorded in §14: the dev proxy sets
`changeOrigin: true` (`web/vite.config.ts`), so how a session cookie is scoped
through it is **unverified**.

## 2. Two hashing algorithms, chosen by entropy

This is the decision most likely to be got wrong by treating "hashing" as one
thing, and getting it wrong is not a style issue.

| Secret | Hash | Why |
| --- | --- | --- |
| Account password | **argon2id** | low entropy, human-chosen. Slowness *is* the defence |
| Session token | **sha256** | 256 bits from a CSPRNG. Nothing to brute-force |
| API key secret | **sha256** | same |

**Argon2 on an API key would be a serious performance defect, not a
conservative choice.** Every page request from an OPDS client carries the
credential, so a deliberately slow KDF at ~100 ms would put roughly **40 seconds
of pure hashing on a 400-page volume**. Fast hashing is correct for a
high-entropy secret precisely because there is nothing for slowness to buy: a
256-bit random token has no dictionary and no structure to attack.

**Both token kinds are stored hashed and never raw**, so a database leak yields
no usable session and no usable API key — the same reason a password is not
stored raw, applied to the two secrets that are easier to forget about.

**API keys carry a `tyk_` prefix, and that is load-bearing rather than
cosmetic.** Basic auth's password field accepts either a password or a key, and
without a discriminator the server would have to attempt an argon2 verify on
every OPDS request before falling back — which reintroduces the exact cost the
table above exists to avoid. The prefix routes the credential to the right
verifier in one comparison.

`argon2id` parameters start at OWASP's current recommendation — m=19456 KiB,
t=2, p=1 — and are **measured on the host that will run this** before shipping.
They are unverified anywhere today (§14) and §12's step 2 is where they stop
being. A parameter set that is fast on a workstation and slow on the server is
the failure this project's measurement discipline exists to catch.

## 3. Migration 2 — **migration 4** *(corrected 2026-08-18)*

`002_absorbed.sql` and `003_scanner.sql` both landed before this branch, so what
this section describes is `004_auth.sql` and `SCHEMA_VERSION` is 4.

**One thing this section does not say, and the implementation had to decide.**
Migration 1 seeds `(1, 'local', 'admin')` with a NULL `password_hash`, so "NULL
-> NOT NULL" needs an answer for that row rather than a constraint it violates.
It gets `'!'` — `credential::LOCKED` — which is not a well-formed PHC string, so
it verifies against nothing, the way `/etc/shadow` refuses a locked account.
`account::admin_exists` reads that placeholder rather than the role, which is
what makes §5's claim route reachable on a fresh library: the seeded row *has*
role `admin`, and counting it would shut the route before anybody could use it.

**And the rebuild itself was more dangerous than this section says.** "Getting it
wrong silently drops the rows it copies" understates it: `PRAGMA foreign_keys` is
a no-op inside a transaction, `apply_pending` runs every migration inside one,
and `DROP TABLE` on a parent fires `ON DELETE CASCADE` — so the obvious version
copies the `user` rows perfectly and deletes every note and reading position.
Measured at zero notes surviving. See `docs/notes.md`.

```sql
ALTER TABLE user ADD COLUMN can_download INTEGER NOT NULL DEFAULT 1;
ALTER TABLE user ADD COLUMN can_stream   INTEGER NOT NULL DEFAULT 1;

-- password_hash NULL -> NOT NULL. SQLite cannot alter a column's nullability
-- in place, so this is the documented twelve-step rebuild: create the new
-- table, copy, drop, rename, inside one transaction with foreign keys off.
-- Called out because it is the only migration here that is not one line, and
-- because getting it wrong silently drops the rows it copies.

session (token_sha256 PK, user_id, created, last_seen, expires,
         user_agent, ip)

api_key (id PK, user_id, name, secret_sha256 UNIQUE,
         created, last_used, expires NULL)

user_root (user_id, root_id, PK(user_id, root_id)) WITHOUT ROWID

login_attempt (key PK, window_start, count) WITHOUT ROWID
    -- `key` is the (client address, username) pair joined, per §7 — not the
    -- account alone, so one misconfigured OPDS client locks out only itself.
```

`session` is keyed by the token's hash rather than carrying a separate id, so
there is no second identifier and no way to store the raw token by accident —
the column that would hold it does not exist.

`api_key.name` is what a person calls the device, so revoking the right one does
not require guessing.

## 4. The guard

One `axum` middleware resolves every request to a `Principal`:

```rust
struct Principal { user_id: i64, role: Role, can_download: bool,
                   can_stream: bool, roots: Vec<i64> }
```

from either the session cookie or `Authorization: Basic`. `/api/*` and `/opds/*`
both sit behind it.

**The 401 differs by path, deliberately.** `/opds` answers
`WWW-Authenticate: Basic realm="taiyaku"`, which is what makes an OPDS client
prompt for credentials. `/api` **must not**: that header makes the browser raise
its own native auth dialog over the reader, which the web app cannot style,
cannot pre-fill and cannot recover from. `/api` returns a plain JSON 401 and the
reader routes to its own login screen.

**Session cookie:** `taiyaku_session`, `HttpOnly`, `SameSite=Lax`, `Path=/`,
`Secure` when a trusted proxy is configured (§6), 30-day expiry, slid forward on
use. `Lax` rather than `Strict`: `Strict` breaks following a link into the app
from anywhere else, and the gap it closes is covered by the origin check below.

**CSRF.** Header auth is not CSRF-able, so `Authorization: Basic` needs nothing.
The cookie path does: every state-changing method — `PUT`, `PATCH`, `POST`,
`DELETE` — requires an `Origin` matching the server's own. Browsers send
`Origin` on all of those; a request without one is refused rather than assumed
same-origin, since an absent header is precisely what a forged cross-site form
produces.

**Session lifecycle.** Logout deletes the row. Changing a password deletes every
session **except the current one**, so a compromised session cannot outlive the
password change that was meant to end it. API keys are *not* revoked by a
password change — they are independent credentials with their own revocation,
and bundling them would make rotating a password silently break every reader in
the house. Expired rows are deleted lazily on lookup and swept at startup.

## 5. Bootstrap, and the row that must be promoted rather than created

**By default, from the CLI:**

```
taiyaku library user promote [<name>] --admin      # prompts for a password
```

No window in which an unclaimed server is reachable, no second code path, and
the host already has a shell.

**Optionally, from the browser, behind a flag:** `taiyaku serve
--allow-web-setup` exposes an unauthenticated claim route **only while no admin
exists**. It closes the instant one does. It logs loudly at startup and on every
request it serves, because a flag whose danger is invisible while it is doing
nothing is a flag that stays on.

**The constraint that matters more than either.** Spec A inserts one implicit
local user and attaches every note, position and read count to it. Whichever
path runs, it must **promote that row** — same `user.id` — and not insert a new
one. Inserting would orphan everything recorded between A and A2 on an account
nobody can log in as, and the loss would be silent, because the reader would
simply show an empty library and a fresh position. A test asserts the id is
preserved (§10).

**Password reset without a mail server.** No email is sent and no self-service
reset exists. An admin resets another user's password; the CLI resets an
admin's. That is a real limitation and is stated rather than designed around,
because designing around it means running a mail server.

### 5.1 Export becomes ambiguous the moment a second user exists

**Recorded here on 2026-08-14, from spec A's implementation, because this is the
spec that creates the problem.**

`export::work` writes a work's five sibling documents, two of which — `notes` and
`progress` — are **per user**. It resolves that today by exporting user 1's,
which is correct while migration 1 seeds exactly one user and no login exists.
The constant carries its reasoning at its definition site.

**A2 makes that silently lossy.** With two users holding notes on one work,
`export::work` writes one of them and says nothing about the other. No error, no
warning, and the sibling file format has nowhere to put a second set — the
documents predate users entirely and have no concept of one.

So A2 must decide what export *means* under multiple users, and the options are
not equivalent:

- Export the **requesting** user's, and say so in the file or the output. Cheap,
  and it makes a shared library's exports personal rather than canonical.
- Export **every** user's, which the current file format cannot express without
  a version bump to `<stem>.notes.json` and `<stem>.progress.json` — and those
  are consumed by `taiyaku-serve`'s existing routes and by the reader.
- **Refuse** when more than one user has state for the work, naming them. Honest,
  and it makes the ambiguity the person's problem at exactly the moment it
  becomes real.

Nothing here is decided. What matters is that it is decided *before* the second
user exists, because after that every export is quietly dropping something.

***(decided 2026-08-18: the first, with the second's honesty.)*** The database
was already multi-user — `note` and `progress` have been keyed
`(user_id, work_id)` since migration 1 — so only the *file* is single-user, and
that is what a sidecar is rather than a limitation to version away. `export::work`
takes a `user_id` and `Exported` gained `other_users`, naming every reader whose
state is not in the files it wrote.

Reporting rather than refusing follows the rule `suppressed`, `rekeyed`, `merged`
and `kept_editions` were all built on: any outcome that changes what a person
sees and cannot be inferred from the other counts gets a field. Versioning the
sibling formats was declined — both are consumed by `taiyaku-serve`'s routes and
by the reader, so the blast radius is the whole round trip. `taiyaku library
export` gained `--user NAME`, defaulting to the lowest account id.

## 6. The trusted proxy is a decision, not a config line

`serve` needs to know it sits behind a proxy for two independent reasons, and
each has a failure mode in both directions:

| | Trusting too much | Trusting too little |
| --- | --- | --- |
| `X-Forwarded-For` | any client spoofs its address and the rate limiter is useless | the whole household is one address and one person's typo locks out everyone |
| Cookie `Secure` | — | the session cookie travels in clear over any non-TLS hop |

So it is **`--trusted-proxy <CIDR>`, repeatable**, rather than a boolean:
`X-Forwarded-For` is honoured only when the *direct peer* falls inside one of
the given ranges, and ignored otherwise. Configuring any trusted proxy implies
`Secure` on the session cookie, with `--insecure-cookies` to override for a
deliberate plain-HTTP deployment on a private network.

A boolean `--behind-proxy` was rejected for the first row of that table: it would
have to trust `X-Forwarded-For` from whatever connected, which on a
misconfigured deployment means trusting the internet.

## 7. Rate limiting

On the login route and on failed `Authorization: Basic` verification only.
Keyed on **(client address, username)** rather than on the account, with a fixed
window and progressive lockout.

The pair rather than the account is deliberate: OPDS clients retry hard, and a
single misconfigured reader that keys on the account alone would lock its owner
out of every device at once. Keyed on the pair, the misconfigured client locks
out only itself.

## 8. Per-library sharing, and an ordering consequence

Permissions are `role` (admin/user), the two capability flags, and `user_root`.
Deliberately **not** Komga's full model: no age ratings, no content
restrictions, no per-label filters. Those exist for libraries shared with
children and were declined.

**Filtering by root needs a column spec A does not have.** In spec A works
arrive by `taiyaku library import`, not by a scan, so no work belongs to a root;
`work.root_id` is scanner-derived and belongs in **spec B**, which lands *after*
this one.

So **A2 ships the mechanism and the filter is inert until B populates it.**
Between A2 and B every authenticated user sees every work, which is correct
rather than a hole: in that window every work in the database was imported by
hand by the admin. Recorded here so it is a known window rather than a
discovered bug, and §10's route test covers the guard whether or not the filter
does anything yet.

***(corrected 2026-08-18)* The ordering reversed, and the conclusion survived
for a different reason.** Spec B landed *before* this one — and added no
`work.root_id`, nor any other work-to-root link: `scan_entry.root_id` and
`rule.root_id` exist, `member` has no `scan_entry_id`. So the filter is still
inert, but not because B has yet to populate it: because nothing populates it,
and because `serve` is handed sidecar paths and no route lists by root, so there
is nothing for a filter to filter.

What shipped is `user_root`, and a guard that resolves `Principal.roots` from it.
Spec C1's browse API is the first caller that can act on the answer. Adding
`work.root_id` here was declined: it would have reproduced O45's exact shape — a
column written on every scan and read by nothing — in the branch that opened
O45's neighbours.

## 9. What the reader gains, and what it does not

**A2 ships:** a login screen, logout, and an account screen — change password,
create an API key (shown once), list and revoke keys, list and revoke sessions.
`api.ts` gains 401 handling that routes to login rather than throwing.

**A2 does not ship user administration.** Creating users, granting roots and
setting capabilities go to **spec C4**, the configuration UI, alongside roots,
scan control and spec B's rule editor. The CLI covers them in between — which it
must anyway, since §5 puts bootstrap there.

**No dev bypass and no `--no-auth` flag.** A 30-day sliding session is one login
per browser per month, and the test suite drives `build_router` with `oneshot`,
so it constructs principals directly and never logs in. A bypass flag would be
the first real footgun on an internet-facing server, and the convenience it buys
is a login screen once a month.

## 10. Testing

**The one worth naming: every route 401s unauthenticated, enumerated from the
router rather than from a list.** A reviewer's checklist is not a test — it goes
stale the first time someone adds a route and does not think about auth, which
is exactly the failure this is guarding. Driving the enumeration off the router
itself means a new unguarded route fails the suite on the commit that adds it.

Also:

- Basic accepted with a password **and** with a `tyk_`-prefixed API key, and the
  key path asserted **not** to reach argon2 — §2's whole point is unobservable
  otherwise, and a later refactor could quietly reintroduce it.
- `WWW-Authenticate` present on an `/opds` 401 and **absent** on an `/api` one.
- Cookie flags: `HttpOnly` and `SameSite` always; `Secure` present with a
  trusted proxy configured and absent without one.
- The session token in the database is not the token in the cookie — asserted by
  inequality, so storing it raw fails rather than passing by inspection.
- A state-changing request with no `Origin` is refused; with a foreign `Origin`,
  refused; with the server's own, allowed.
- `X-Forwarded-For` honoured from inside a configured CIDR and ignored from
  outside it.
- **Promotion preserves `user.id`**, with a note and a read position attached
  before promotion and asserted still attached after (§5).
- argon2id cost measured and asserted to be within a band, not merely correct —
  a parameter set that verifies correctly and takes 2 ms is the defect.

## 11. What is deliberately not done

- **No TLS in `serve`.** A reverse proxy terminates it. §6 is the whole of what
  `serve` knows about it.
- **No OIDC, no SSO, no OAuth provider.**
- **No self-service password reset and no email**, per §5.
- **No user self-registration.** An admin creates accounts.
- **No 2FA.** Not refused on principle, just not asked for and not free.
- **No age ratings or content restrictions**, per §8.
- **No per-root filtering in practice** until B, per §8.
- **No admin UI**, per §9.

## 12. Branch shape

One branch, one squashed PR. Ordered so the risky claims are tested first:

1. Migration 2, including the `password_hash` table rebuild — on its own,
   because it is the step that can lose rows.
2. `argon2id` and `sha256` credential verification, with §2's cost measurement.
3. The `Principal` middleware, the two 401 shapes, and §10's route enumeration
   test — before any route depends on it.
4. Cookie sessions, CSRF origin check, session lifecycle.
5. `--trusted-proxy`, rate limiting.
6. CLI: `user promote`, `user add`, `user password`, `key create`, `key revoke`.
7. `--allow-web-setup` and the claim route.
8. Web: login, logout, account screen, `api.ts` 401 handling.

***(corrected 2026-08-18)* Four steps this list does not name, all of which the
work needed.** The two `LOCAL_USER = 1` constants — `taiyaku-library`'s
`export.rs` and `taiyaku-serve`'s `works.rs`, which this design knew about as one
— had to be deleted and threaded through as an argument, which is two steps of
its own; §5.1's decision above had to be implemented rather than only made; §14's
checkable unknowns had to be checked; and the documents §13 lists are four edits
where this project's open-items ritual is more than four.

Step 1 also grew a piece nothing here anticipated: the foreign-key pragma had to
move out of the transaction that made it a no-op, which is a change to
`schema.rs` rather than a `.sql` file.

## 13. The measurements this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs`
- `docs/status.md` — the crate table's `taiyaku-serve` row, the test count, and
  the dependency list, which gains `argon2`
- `README.md` — the `taiyaku serve` synopsis, which gains `--trusted-proxy`,
  `--insecure-cookies` and `--allow-web-setup`, and "Using it", where every
  example now needs a credential
- `web/README.md` — how to run the reader against an authenticated `serve`

## 14. What could not be measured

***(measured 2026-08-18, two of the three.)*** The first is still owed and the
second is half-answered; the third was measured, misbehaved exactly as this
section allowed for, and was fixed the way this section said to fix it. All three
are written up in `docs/notes.md`, "Spec A2's measurements".

- **argon2id parameters on the host that will run this.** OWASP's numbers are a
  starting point, not a measurement; §2's are unverified anywhere and §12 step 2
  is where they stop being.
  *Measured on the workstation: 11.92 ms to hash, 10.93 ms to verify, at OWASP's
  parameters. **The deployment host is still unmeasured**, which is the whole of
  what this item asked for — a parameter set fast here and slow there is the
  failure the discipline exists to catch. The cost assertion deliberately does
  not use a clock floor: one set near 23 ms fails on faster hardware and reports
  a hardware upgrade as a security defect, so the parameters are asserted as
  three `const` assertions instead, with a 2 ms floor for "something stopped
  calling argon2 at all".*

- **Whether Panels accepts an API key in the Basic password field.** The whole
  design of §2's prefix rests on OPDS clients treating that field as opaque,
  which is what the specification implies and what no test here has confirmed
  against the actual client. It is checkable in one attempt against the real
  reader and should be, before the account-screen UI promises it.
- **How a session cookie is scoped through the dev proxy.** `vite.config.ts`
  sets `changeOrigin: true`, which rewrites the `Host` header toward `serve`. A
  host-only cookie should come back correctly scoped to `localhost:5173`, and
  whether `Secure` is honoured on `http://localhost` varies by browser. Neither
  was tried. If it misbehaves, the fix is in the dev proxy and not in the
  cookie policy — a production cookie must not be weakened for a development
  convenience, which is the same rule `vite.config.ts` already states about
  choosing a proxy over CORS.

  ***Measured 2026-08-18: it misbehaved, and the fix went where this paragraph
  said.*** `POST /api/session` through `pnpm dev` answered **403** — the proxy
  rewrites `Host` to the target while `Origin` stays `localhost:5173`, so §4's
  check compared two different authorities. Every login through the dev server
  failed. `changeOrigin` is now `false`, so the proxy forwards the `Host` the
  browser sent; nothing needed the rewrite, since the target is this project's
  own `serve`, which routes on the path and reads `Host` only to compare it
  against `Origin`. The cookie policy is unchanged. `Secure` is correctly absent
  over plain HTTP with no `--trusted-proxy` configured.

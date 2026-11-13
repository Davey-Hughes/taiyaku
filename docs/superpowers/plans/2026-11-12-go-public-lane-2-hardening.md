# Go-public lane 2: server hardening (O277, O278, O279) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close lane 2 of the go-public design before the public name opens.
- O277: every answer carries four security headers, and the app runs no inline script.
- O278: the login limiter keys an IPv6 client by its /64, after reading a mapped IPv4 address as its plain form.
- O279: one account's wrong passwords are counted across addresses. At 30 in one window they are logged once and marked on Users, and nothing is locked.

**Architecture:**
- Task 1: the theme script moves from `web/index.html` to `web/public/theme.js`.
- Task 2: one `from_fn` layer on the whole router sets the headers, and the README says so.
- Task 3: one library function writes the address half of every limiter key.
- Task 4: a third `login_attempt` key, per account, written after the two limiter writes without waiting, and a stderr line at the ceiling.
- Task 5: the window's count on the Users answers, marked by `UserTable.tsx`.
- Task 6: the closes in `docs/status.md`.
- Then the Finish, whose CSP hand-check runs a script-launched Playwright Chromium against a served build.

One PR (ruled). No migration.

**Tech Stack:** Rust (axum 0.8, rusqlite), TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`, and Playwright (in `web/node_modules`) for the hand-check.

**Spec:** `docs/superpowers/specs/2026-11-12-go-public-design.md`: §4 (lane 2), §7 (overlaps, merge order) and §9 (proof), cited as "spec §n". Locations are by symbol. Line numbers are hints at main ec4eb11c. The survey behind §4 is `.superpowers/sdd/deploy-evidence/survey-hardening.md`, sections B and C.

**The owner's rulings of 2026-11-12** (`.superpowers/sdd/deploy-evidence/rulings.md`; binding, do not re-open):
- One PR with three parts. The security headers. The IPv6 key at /64, with a mapped IPv4 address canonicalised first. A per-account ceiling that is an ALARM ONLY: it counts per account, logs and shows past a ceiling, and locks nothing. Auth spec §7's pair lockout stays.
- The builder's calls, told to the owner:
  - The theme script moves to a file, so the policy allows no inline script.
  - Framing is denied.
  - HSTS is left to NPM and Cloudflare; NPM already sends it.
- The spec's own calls (§4 decisions 5 and 6):
  - One layer puts the headers on every answer, enforced from the start.
  - The ceiling is 30 failures in one 15-minute window, logged once and shown on Users.
- Anti-goals (spec §2) that no task reverses:
  - The lockout stays keyed on (address, name), never on the account.
  - O57's reads stay as they are, and nothing new is read before a verify.
  - A write the request did not ask for is tried once without waiting (O272).

**What the plan settles** (spec §8 left these to the builder):
- **The theme file** is `web/public/theme.js`, served at `/theme.js`.
  - Vite copies `public/` unhashed. Its HTML plugin leaves alone a classic `<script src>` that names a public file (read in Vite 8.2.1).
  - `web::Web::answer` serves it with `REVALIDATE`.
- **The header layer** is `routes::security_headers`, an `axum::middleware::from_fn`, so no new `tower-http` feature is needed.
  - It inserts the four headers on every answer and overwrites any already set. No handler sets one today (grep), and a weaker one set later must lose to the policy.
- **The address half** has its home at `taiyaku_library::account::limiter_address`, beside `attempt_key` and `address_key`, whose collision rules it has to keep.
  - An IPv6 /64 renders as `<network>/64`, e.g. `2001:db8:1:2::/64`.
- **The alarm's key** is `account {id}` (`account::account_key`).
  - It holds no newline, so it is never a pair key or a password-change key.
  - It holds a space, which no address half does.
- **Users' field** is `wrong_passwords`.
  - Beside it the server sends `wrong_password_alarm: bool` (the count has reached `account::ALARM_AFTER`), so the web never holds the ceiling (the controller's call, 2026-11-12, over mirroring it).
- **The exact words.** A person reads these, and tests hold them as literals.
  - stderr, once at the ceiling, here for an account `sam` with id 2:
    `WARNING: 30 wrong passwords for the account "sam" (id 2) in 15 minutes. Nothing is locked, and the Users screen marks the account until those 15 minutes are up.`
    - The name is printed with `{:?}`, so a quote or a newline in a name cannot forge or split the line.
    - 30 and 15 come from `ALARM_AFTER` and `LOCKOUT_WINDOW_MINUTES`.
  - On Users, after the name, and after "locked" where both apply: `{JOINING_DOT}<n> wrong passwords in 15 minutes`.
  - Tasks 2 and 5 give the README's sentences.
- **Tests set a client's address** with a `ConnectInfo<SocketAddr>` on the request. `auth::client_of` reads the request's extensions on all three paths, so no trusted proxy is needed in-process.
- **The CSP hand-check uses a script-launched Playwright Chromium**, where spec §9 names chrome-devtools. This is the controller's instruction: parallel lanes' drives collided in the shared MCP browser before.
- **No task adds production SQL beyond a `SELECT`.**
  - The alarm reuses `note_failure`'s upsert.
  - The tests' trigger has the shape of one `session/tests.rs` already runs in CI (`a_claim_fault`).
  - So no `rust:1-bookworm` run is planned. A task that does add SQL runs that crate's tests there before the Finish, and says so.

**New interfaces this plan names:**
- Task 2, in `routes.rs`:
  - `CONTENT_SECURITY_POLICY: &str`.
  - `async fn security_headers(request: axum::extract::Request, next: axum::middleware::Next) -> Response`.
- Task 3: `account::limiter_address(address: std::net::IpAddr) -> String`.
- Task 4, in `account.rs`:
  - `ALARM_AFTER: i64 = 3 * LOCKOUT_AFTER`, which is 30.
  - `account_key(id: i64) -> String`.
  - `note_account_failure(conn: &Connection, id: i64) -> Result<bool, String>`: true exactly when this failure brings its window's count to `ALARM_AFTER`.
  - `account_failures(conn: &Connection, id: i64) -> Result<i64, String>`: the window's count, 0 when there is none or it has lapsed.
- Task 4, in `session.rs`, placed after `verify_holding`:
  - `count_wrong_password(conn: &rusqlite::Connection, account: &taiyaku_library::account::Account)`.
  - `alarm_line(name: &str, id: i64) -> String`.
- Task 5:
  - `users_api::user_json` takes `wrong_passwords: i64` and sends `"wrong_passwords"`.
  - The web gets `User.wrong_passwords: number` and `User.wrong_password_alarm: boolean`; the Rust test holds accounts at 29 and exactly 30 failures and asserts `wrong_password_alarm` false and true.

## Spec gaps

Each of these was found against the code. None is resolved silently; each says what the plan does.
1. **§9's "print one line" cannot be asserted in-process.** `auth::ignored_forwarded_warning`'s doc says the same of its own `eprintln!`.
   - The plan splits the decision from the print. The decision is `note_account_failure`'s answer, tested at 29, 30 and 31 and after a lapse. The text is `alarm_line`'s, tested.
   - The drive in the Finish reads the real stderr.
2. **§9 expects `session/tests.rs:581` to pass unchanged, and it does.** But two O57 tests pin a real account's wrong password at two commits, both under the permit: `a_login_with_a_wrong_password_records_its_failure_holding_its_permit` and `a_basic_request_with_a_wrong_password_records_its_failure_holding_its_permit`.
   - The alarm's write is a third commit, so Task 4 moves both to three.
   - That change is also what holds §4's "once" and "after the two limiter writes".
3. **§4 has `UserTable.tsx` mark at the ceiling, but the server sends only the count.** SETTLED by the controller: the server also sends `wrong_password_alarm: bool`, so the ceiling lives only in `account::ALARM_AFTER` and cannot drift.
4. **§4's "without waiting" has no affordable test.** Under a held write lock, the two limiter writes before the alarm each wait out the busy timeout. A timing test would therefore take two busy timeouts to see a third. The reviewer checks it by reading.
5. **A new account can inherit a removed account's count.** `user.id` is `INTEGER PRIMARY KEY` with no `AUTOINCREMENT`. So an account made within 15 minutes of removing the highest-id account inherits its count, and may be marked for one window. The spec is silent. The plan accepts this and adds no code.
6. **`004_auth.sql`'s comment on `login_attempt` says its key is "NOT the account alone".** The alarm adds an account key, which is a count and not a lockout. Applied migrations are not edited, so `account_key`'s doc says this instead.

## Global Constraints

- **Paths.**
  - `W=/home/davey/projects/taiyaku/.claude/worktrees/go-public-2` is this lane's worktree. Its branch is cut from main at BASE, the merge of the spec PR (provisionally ec4eb11c).
  - `E=/home/davey/projects/taiyaku/.superpowers/sdd/deploy-evidence/lane-2` is absolute and in the MAIN checkout, so reports survive the worktree.
  - Every command names `$W` by absolute path. Never `cd`: it is zoxide here. Where a tool must run from a directory, write `builtin cd <dir> && …` within that one call.
- **Before Task 1.**
  - `$W/crates/taiyaku-wasm/pkg` exists: copy it from the main checkout, or run `$W/scripts/build-wasm.sh`.
  - `$W/web/node_modules` is a real copy, never a symlink.
- **Three lanes run at once**, each in its own worktree. Never set `CARGO_TARGET_DIR`: each worktree builds into its own `$W/target`. Never `--amend`, and check each new commit hash is an ancestor of HEAD.
- **Shared files** (spec §7). At rebase, `git diff` of each shared file must show only this lane's functions changed.
  - `session.rs`:
    - This lane: `login`, `change_password`, and the new `count_wrong_password` and `alarm_line`, placed after `verify_holding`.
    - Lane 3 (spec §5): `refuse_a_device_credential`, `administering_or_forbidden`'s doc and `whoami`.
  - `auth.rs`:
    - This lane: `from_basic` only.
    - This lane adds no test to `auth/tests.rs`, whose route-list tests lane 3 edits.
  - `session/tests.rs`: this lane's new tests go at the end, plus the two O57 tests named in Task 4.
  - `routes.rs`:
    - This lane: `build_router_serving` and the new `security_headers`, beside `per_reader_by_default`.
    - Lanes 3 and 4 edit handlers.
  - `README.md`:
    - Lane 1 edits the "Using it" proxy comments, the compose block, the restore steps and the add's scan.
    - This lane adds one paragraph and one sentence after the NPM bullets.
  - `docs/status.md`: every lane's last task. Redo the counts at rebase.
- **Merge order 1, 2, 3, 4.** This lane rebases onto main after lane 1 merges, and lane 3 rebases onto this one.
- **Gate.** Run the repo's own script, never a copy or a modified one.
  - Command: `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`.
  - It must start inside `$W`, because the script `cd`s to `git rev-parse --show-toplevel`.
  - Wait for load < 24 (`cat /proc/loadavg`) and record the load.
  - Read the `.exit` file, never the harness's exit code, and quote the `all green` line.
  - Never pipe the gate through `tail`.
- **Rust, per task.** Run whole crates, never a named test, and read the summary lines from the log.
  - Tests: `cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/T<n>-nextest-<crate>.log 2>&1`.
  - Then `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings`.
  - Then `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`.
- **Web, per task.** Run whole projects, never a named file, and never `pnpm exec`.
  - Tests: `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`.
  - Then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`, the same way.
  - A task that touches a `*.browser.test.tsx` file, or CSS, runs the whole browser project. CI never runs it.
  - A browser failure with import or connection errors and no failed assertion is headless Chromium crashing. Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at BASE, not at another commit of this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one.
  - Put the named defect back by hand, see the test fail, restore, and check the restore with `git diff`.
  - Record the defect, the failing test and its message in the task report.
- **Literals.** A test asserts a user-facing sentence, a header value or the policy written out in the test, never one computed by the code under test.
- **Comments.**
  - A comment says why: the reason, the invariant, what breaks if it changes. No history and no measurements; it may cite an O-number.
  - `scripts/check-comments.mjs` and `scripts/comment-budget.json` enforce part of this in the gate. If a share fails, follow `docs/ci.md`, "The comment guard", and say so.
  - No AI attribution anywhere.
- **Coverage floors.** If a per-file floor fails in the gate, decompose it per `docs/ci.md` before touching a floor, and say so.
- **Commits and reports.**
  - One commit per task, its subject naming the O-number(s).
  - Each task writes `$E/T<n>-report.md` by Bash (`cat >| … <<'EOF'`). The Write tool is refused under `.superpowers/`. Never write reports under `target/`.
- **The test count.** `docs/status.md` gives the Rust total twice, in "Six crates, N tests" and "of the N are standing checks", and the gate's `status-test-count` step checks both. Task 6 sets both from a whole-workspace run; the tasks before it leave them as they are.

## Review Focus

The five failure modes most likely to reach a person that the spec implies and no unit test sees, most likely first:
1. **A built page that the policy breaks.** Examples: the wasm, the `data:` fonts, a download link, or anything Vite adds to the built HTML. The browser refuses it and the page half-works. Finish step 2 drives the built app, with a control violation.
2. **A proxy or Cloudflare feature that adds a script, or a second policy, through the public name.** Finish step 8 covers it, and so does Task 2's README sentence.
3. **A trusted proxy on a dual-stack listener that forwards a mapped `::ffff:a.b.c.d`.** It must share a key with the same client's plain form, and not share `::/64` with every other IPv4 client. Task 3's mapped route test covers it.
4. **A renamed account.** It keeps its count, because the key is the id. Task 4's rename test covers it.
5. **The alarm's write refused under a derive's lock.** It is dropped, and the answer is unchanged: 401, not 429. Task 4's trigger test covers that. That the write is made without waiting is checked by reading (spec gap 4).


## Pre-flight rulings (controller, 2026-11-12; binding, override the task text where they differ)

From the pre-flight scan `.superpowers/sdd/deploy-evidence/lane-2/preflight.md` (0 BLOCKER, 2 Important, 17 Minor). BASE is **28b18b60** (the spec PR's merge; docs-only over ec4eb11c).

- **Task 1.** Files also include `web/tests/theme.test.ts` ~42-44, whose comment "`index.html`'s pre-paint script spells it out" goes stale (now `theme.js` spells the key). The order test also asserts the `theme.js` script's index is below `indexHtml.indexOf('</head>')`; named mutation: move it into `<body>` above the module script — the test fails.
- **Task 2.** The policy value is named `SECURITY_POLICY` (not `CONTENT_SECURITY_POLICY`, which sits beside `header::CONTENT_SECURITY_POLICY` in the same function). The written-out policy literal the tests assert lives once in `test_support` and both test files use it (still a literal, never the code's const). The wrong-password `POST /api/session` step sends `host` and `origin` headers: without them `refuse_cross_site` answers 403 first.
- **Task 3.** The hand-restore wording is "`&address.to_string()` / `&client.to_string()`" (`from_basic`'s two sites use `client`). Named mutation for the widened `the_wide_key_cannot_collide_with_a_pair`: `limiter_address` rendering `"{network} /64"` fails the no-space assertion.
- **Task 4.** Named mutation for `the_account_key_cannot_collide_with_a_pair_or_an_address`: `account_key` as `format!("account\n{id}")` fails the no-newline assertion. Spec gap 2's wording: the three-commit vector proves the alarm's write happens **once** under the permit; its **order** (after the two limiter writes) is checked by reading, as gap 4 says.
- **Task 5 (I1, Important).** The Rust test also winds two accounts (or `a_reader` plus one more) by `note_account_failure` to 29 and to exactly 30, and asserts `wrong_password_alarm` false at 29, true at 30, false for account 1 at 3, and false in the lapse case. The `>=`→`>` mutation then fails on the account at exactly 30.
- **Task 5 (I2, Important).** The jsdom fixture states each row's `wrong_password_alarm` (sam 30 true, kim 29 false, lee 31 true) and adds a row whose flag and count disagree: `max` at 30 with `wrong_password_alarm: false`, expected unmarked (`'max'`). Named mutation: mark where `wrong_passwords >= 30` instead of reading the flag — max's row fails.
- **Task 5 (Minors).** Delete "the constant" from "Implement the field, the type, the constant, the mark…" (the web has no constant; `ALARM_AFTER` is Task 4's, server-side). Expected `td.lead` texts spell `JOINING_DOT` as it is (`' · '`, NBSP·NBSP), e.g. `'sam · 30 wrong passwords in 15 minutes'`.
- **Task 5 ("15 minutes").** Ruling: the mark's "15 minutes" stays a web literal; `users.ts`'s doc for the mark names `LOCKOUT_WINDOW_MINUTES` in `crates/taiyaku-library/src/account.rs` as the figure it repeats. — the controller ruled out a web mirror of the threshold, and the window is a fixed sentence fact, not a decision rule — cost if wrong: one sentence goes stale if the window ever changes.
- **Task 6.** Step 3's sweep also corrects O272's ROW (`docs/status.md` ~1881, "records two failures, the pair's and the address's … each waiting up to 5 s"), not only its notes bullet. The notes bullets go after the last O27x bullet (O276's once lane 1 has merged; O274's at BASE).
- **Finish.** The drive also checks `wrong_password_alarm` true at 31. The shared-file list adds `crates/taiyaku-serve/src/routes/tests.rs` (lane 3 edits `as_the_fixture_account` and adds tests) and `web/tests/Config.browser.test.tsx` (lane 1 adds `schedule_kept` to `Added` fixtures): re-run `tsc -b` and the whole browser project at rebase. Lane 3's `session.rs` touch is `administering_or_forbidden`'s doc only, and `auth.rs` is untouched by lane 3. The policy hand-check of Configuration takes in New library with lane 1's box once lane 1 has merged.
- **Spec gaps (added).** 7: the Finish's CSP hand-check uses script-launched Playwright, where spec §9 names chrome-devtools; the reason is stated under "What the plan settles".

---

### Task 1: The theme script in a file of its own (O277, part 1)

Spec §4 "Security headers", its last two bullets; decision 2.

**Files:**
- Create `web/public/theme.js`: the IIFE now inline in `web/index.html` (~20-29). Its behaviour does not change.
- `web/index.html`:
  - `<script src="/theme.js"></script>` goes where the inline script was: still in `<head>`, and before the body's module script.
  - The old comment's sentence on the `theme-color` pair stays beside the two metas. The rest of the comment moves into the file, rewritten as below.
- `web/tests/theme.test.ts`: the describe "index.html's pre-paint script" (~67-90).
- Four comments that name the inline script: `web/src/theme.ts` (module doc, ~9), `web/src/prefs.ts` (~25), `web/src/main.tsx` (~9-12) and `web/src/ThemeControl.tsx` (~8). `web/src/label.tsx`'s (~26) stays true; leave it.
- `docs/superpowers/plans/2026-11-12-go-public-lane-2-hardening.md`: this plan, copied from `.superpowers/sdd/deploy-evidence/plans/` and committed with this task.

**The file's comment says:**
- It applies the stored theme before first paint (spec 2026-09-17 §3). Without it, an Ink reader on a light-mode OS sees paper while the modules load.
- It is a file and not inline because the server's policy runs no inline script (O277). A classic script in `<head>` still runs before the body is parsed.
- `theme.test.ts` runs it against `loadTheme` and `applyTheme`.
- It is wrapped in a function called at once, so `choice` is a local and not a global on `window`.

`main.tsx`'s comment now says the theme is applied again there so that a `/theme.js` which failed to load costs a flash, not the theme.

- [ ] **Step 1: Write the failing node tests** in `theme.test.ts`.
  - The `it.each(STORED)` agreement cases run `../public/theme.js?raw` (imported the way `index.html?raw` is) instead of the inline match.
  - 'comes before the app module': `indexHtml.indexOf('<script src="/theme.js"></script>')` is above -1 and below `indexHtml.indexOf('<script type="module"')`.
  - New, "index.html and label.html hold no inline script, which the server's policy refuses": every `<script` opening tag in either file carries `src=`. Import `../label.html?raw` for it.
  - Run the node project. The new and changed cases fail.
- [ ] **Step 2: Move the script and its comment, and update the four comments.** Run the node and jsdom projects to green.
- [ ] **Step 3: Watched mutations.**
  - Put an inline `<script>` back into `index.html`'s head: the no-inline test fails.
  - Move `<script src="/theme.js">` below the module script: the order test fails.
  - Change `'ink'` to `'dark'` in `theme.js`: an agreement case fails.
- [ ] **Step 4:** Run `tsc -b` and `biome check`. Commit `… (O277)` and write `$E/T1-report.md`.

---

### Task 2: Security headers on every answer (O277, part 2)

Spec §4 "Security headers", decisions 3-5; §9's header tests for lane 2.

**Files:**
- `crates/taiyaku-serve/src/routes.rs`:
  - Add `CONTENT_SECURITY_POLICY` and `security_headers` beside `per_reader_by_default` (~2990).
  - In `build_router_serving` (~3016), add `app = app.layer(axum::middleware::from_fn(security_headers))` after the `--web` fallback block (~3073-3076) and before the log layer (~3078).
- `crates/taiyaku-serve/src/web/tests.rs`: `built_app` (~21) also writes `dist/theme.js`; add a new test.
- `crates/taiyaku-serve/src/routes/tests.rs`: a new test beside `no_document_a_client_can_change_is_given_a_freshness_lifetime` (~2291).
- `README.md`: one paragraph after the bullets that follow the NPM `location` block. That is after "… `docker ps` shows yours)." and before "`TAIYAKU_TRUSTED_PROXY` names …".

**Values** (spec §4, literal):
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: same-origin`
- `X-Frame-Options: DENY`
- `Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; font-src 'self' data:; img-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`

Take the names from `axum::http::header`: `X_CONTENT_TYPE_OPTIONS`, `REFERRER_POLICY`, `X_FRAME_OPTIONS` and `CONTENT_SECURITY_POLICY`.

**Comments:**
- `security_headers`'s doc says why each header is there, and why on every answer. A browser that opens an API or OPDS address directly gets the same policy; OPDS clients are not browsers.
- The comment at the layer says why it goes:
  - after the fallback, since axum's `Router::layer` wraps only what is already registered, so a fallback set later would be outside it;
  - inside the log layer, which then logs what was sent.

**README paragraph.** The wording is the builder's (spec §8); the claim check reads it against the code.
> taiyaku sends `Content-Security-Policy`, `X-Content-Type-Options`, `Referrer-Policy` and `X-Frame-Options` with every answer, so a browser runs only taiyaku's own scripts, styles and fonts, and no other site can show it in a frame. It sends no `Strict-Transport-Security`: TLS ends at the proxy, so HSTS is the proxy's to send. The policy refuses any script the proxy or Cloudflare adds to the page, such as Rocket Loader's, so leave such features off for this host.

- [ ] **Step 1: Write the failing tests.**
  - `web/tests.rs`, `every_answer_the_app_serves_carries_the_security_headers`:
    - Paths: `/`, `/label.html`, `/theme.js`, `/assets/main-Ab12Cd34.js`, `/read/some-work`, `/assets/gone-Zz99Yy88.js` (404) and `/api/nothing-here` (404).
    - For each path, each of the four headers equals its literal value.
  - `routes/tests.rs`, `api_and_opds_answers_carry_the_security_headers`, through `build_router` (no `--web`):
    - An anonymous `GET /api/me`: 401.
    - The same through `test_support::authenticating`: 200.
    - `GET /opds/v1.2/catalog` through `test_support::key_authenticating`: 200.
    - A `POST /api/session` with a wrong password (an open route): 401.
    - `GET /nothing-here` (axum's own fallback): 404.
    - Each status is asserted too, so a case that stopped reaching its route is seen.
  - Run `-p taiyaku-serve`. Both tests fail.
- [ ] **Step 2: Implement.** Run to green, then clippy and fmt.
- [ ] **Step 3: Watched mutations.**
  - Put the layer on `guarded` instead of `app`: the web test fails.
  - Call it before `app.fallback(…)` is set: the web test fails.
  - Drop `'wasm-unsafe-eval'` from the policy: both tests fail.
  - Leave out `X-Frame-Options`: both tests fail.
- [ ] **Step 4:** Write the README paragraph. Commit `… (O277)` and write `$E/T2-report.md`.

---

### Task 3: An IPv6 client is keyed by its /64 (O278)

Spec §4 "The /64 key"; §9's /64 tests for lane 2.

**Files:**
- `crates/taiyaku-library/src/account.rs`:
  - Add `limiter_address` beside `attempt_key` (~602).
  - Docs to update:
    - `attempt_key`'s alphabet sentence (~597-600) and `address_key`'s collision sentence (~642-644) name what `limiter_address` renders: digits, hex letters, dots, colons and a slash, never a newline or a space.
    - `ADDRESS_LOCKOUT_AFTER`'s doc (~605-610) adds that a household on one /64 is one address, as one behind a NAT is.
- `crates/taiyaku-serve/src/session.rs`:
  - `login`: `key` (~491) and `wide` (~496) take their address half from `account::limiter_address(address)`.
  - `change_password`: its `key` (~1102) does the same.
  - `open_session`'s `Some(&address.to_string())` (~583) stays: the session row keeps the whole address.
- `crates/taiyaku-serve/src/auth.rs`:
  - `from_basic`: `key` (~726) and `wide` (~733) do the same.
  - `client_address` and `client_of` are unchanged.
- Tests:
  - `crates/taiyaku-library/src/account/tests.rs`, beside `the_wide_key_cannot_collide_with_a_pair` (~388).
  - `crates/taiyaku-serve/src/session/tests.rs`, at the end.

**`limiter_address`** (spec §4):
- `address.to_canonical()` first, so `::ffff:a.b.c.d` becomes `a.b.c.d`.
- Then an IPv4 address whole, as its `to_string()`.
- An IPv6 address as its /64: the address with its low 64 bits cleared, rendered `"{network}/64"`.

Its doc says why the canonical step comes first: without it, every mapped IPv4 client falls in `::/64`, one key for all of them.

- [ ] **Step 1: Write the failing library tests.**
  - `the_limiter_keys_an_ipv6_client_by_its_64_and_an_ipv4_one_whole`:
    - `2001:db8:1:2::1` and `2001:db8:1:2:ffff:ffff:ffff:ffff` both give `"2001:db8:1:2::/64"`.
    - `2001:db8:1:3::1` gives `"2001:db8:1:3::/64"`.
    - `203.0.113.9` gives `"203.0.113.9"`, and `203.0.113.10` gives `"203.0.113.10"`.
  - `a_mapped_ipv4_address_is_keyed_as_its_plain_form`: `::ffff:203.0.113.9` gives `"203.0.113.9"`, and `::ffff:198.51.100.1` gives `"198.51.100.1"`.
  - `the_wide_key_cannot_collide_with_a_pair` gains a /64 address half: it holds no newline and equals no pair built from it.
    - Its comment's "a rendered `SocketAddr`" becomes what `limiter_address` renders.
- [ ] **Step 2: Write the failing serve tests.**
  - Helpers, in `session/tests.rs`:
    - `from_peer(peer: &str, request: Request<Body>) -> Request<Body>`: inserts `axum::extract::ConnectInfo(SocketAddr::new(peer.parse().unwrap(), 1234))` into the request's extensions.
    - One that reads a key's `count` from `login_attempt`.
  - The fixture account is on `CHEAP_PHC`, set as `a_cheap_password_with_one_failure` sets it.
  - `two_addresses_in_one_64_share_a_login_pair_and_an_address_count`:
    - Five wrong logins from `2001:db8:1:2::1` and five from `2001:db8:1:2::99` answer 401.
    - The next one, from `2001:db8:1:2::abc`, answers 429.
    - The count under `address_key("2001:db8:1:2::/64")` is 10.
    - A wrong login from `2001:db8:1:3::1` answers 401.
  - `two_addresses_in_one_64_share_a_basic_pair_and_an_address_count`: the same shape over `wrong_basic`, the count included.
  - `two_addresses_in_one_64_share_the_password_change_key`:
    - Ten `wrong_change(&cookie)` requests, split across two addresses of one /64.
    - The eleventh, from a third address of it, answers 429.
  - `a_mapped_ipv4_client_and_its_plain_form_share_a_login_pair`:
    - Five wrong logins from `::ffff:203.0.113.9` and five from `203.0.113.9`.
    - The next one, from `::ffff:203.0.113.9`, answers 429.
    - A wrong login from `::ffff:198.51.100.1` answers 401.
  - Run both crates. The new tests fail.
- [ ] **Step 3: Implement.** Run `-p taiyaku-library` and `-p taiyaku-serve` to green.
  - Two existing tests must pass unchanged; the report says so:
    - `lockout_is_keyed_on_the_pair_not_the_account` (`account/tests.rs` ~402).
    - `a_run_of_failures_locks_the_pair_out_and_the_right_password_waits` (`session/tests.rs` ~581).
- [ ] **Step 4: Watched mutations.**
  - Drop `to_canonical`: the mapped tests fail, in the library and at the route.
  - Drop the mask, keying an IPv6 address whole: the /64 tests fail.
  - Mask at /48: `2001:db8:1:3::1`'s case fails.
  - Mask an IPv4 address at /24: `203.0.113.10`'s case fails. A /24 leaves `198.51.100.1` and `203.0.113.9` apart, so the mapped route test's last assertion catches only a mask that merges them (e.g. every IPv4 address to one key).
  - Put each of the five call sites back to `&address.to_string()`, one at a time. Record which test fails for each site.
- [ ] **Step 5:** Run clippy and fmt. Commit `… (O278)` and write `$E/T3-report.md`.

---

### Task 4: One account's wrong passwords, counted across addresses (O279, part 1)

Spec §4 "The per-account alarm"; §9's alarm tests for lane 2.

**Files:**
- `crates/taiyaku-library/src/account.rs`, beside `address_key` and `note_failure` (~630-720):
  - `ALARM_AFTER`: its doc gives spec §4's "Why 30", and says that it refuses nothing.
  - `account_key(id)` is `format!("account {id}")`. Its doc says:
    - the collision rule (see "What the plan settles");
    - that the id keeps the count across a rename;
    - spec gap 6.
  - `note_account_failure(conn, id)` calls `note_failure(conn, &account_key(id))` and answers whether the count it got equals `ALARM_AFTER`. So it answers true once per window.
  - `account_failures(conn, id)` answers the window's count under `account_key(id)`. It shares one windowed read with `spent`, for the reason `spent`'s doc gives: the window arithmetic cannot drift between them.
- `crates/taiyaku-serve/src/session.rs`, after `verify_holding` (~225):
  - `alarm_line(name, id)`: the sentence in "What the plan settles".
  - `count_wrong_password(conn, account)`:
    - Calls `taiyaku_library::without_waiting(conn, |conn| account::note_account_failure(conn, account.id))`.
    - Prints `alarm_line` with `eprintln!` on `Some(Ok(true))`, and drops every other answer.
    - Its doc says the count refuses nothing, and that it is never read before a verify (O57, O215).
    - It also says the write is tried once without waiting (O272), so a failure during a derive's lock goes uncounted, and that a success does not clear it.
- In `login`'s wrong-password branch (the one holding `found`, `if !verify_holding(…)` ~548):
  - `count_wrong_password(&conn, &found)` goes after the two `note_failure` calls, and before the unchanged `pair.is_err() || address.is_err()` decision.
- `crates/taiyaku-serve/src/auth.rs` `from_basic`: the same in its wrong-password branch (`if !crate::session::verify_holding(…)` ~791).
- Three places stay untouched: the unknown-name branches of `login` and `from_basic`, and `change_password`.
- Tests:
  - `account/tests.rs`.
  - `session/tests.rs`: at the end, plus the two O57 tests named in Step 2.

- [ ] **Step 1: Write the failing library tests.**
  - `the_alarm_sounds_once_at_the_thirtieth_wrong_password_in_a_window`:
    - For account 7, `note_account_failure` answers false 29 times, true the 30th time, and false the 31st.
    - `account_failures(7)` is 31, and `account_failures(8)` is 0.
    - With account 7's `window_start` set 16 minutes back, `account_failures(7)` is 0. The next failure answers false, with the count at 1.
    - The numbers are literals, not `ALARM_AFTER`.
  - `the_account_key_cannot_collide_with_a_pair_or_an_address`:
    - `account_key(1)` holds no newline.
    - `limiter_address` of an IPv4, an IPv6 and a mapped address holds no space.
    - `account_key(1)` equals no `attempt_key` and no `address_key` built from those addresses.
- [ ] **Step 2: Write the failing serve tests.** The fixture account is on `CHEAP_PHC`, and addresses come from Task 3's `from_peer`.
  - `thirty_wrong_passwords_from_three_addresses_raise_the_alarm_and_lock_nothing`:
    - Ten wrong logins from each of `203.0.113.1`, `203.0.113.2` and `203.0.113.3` answer 401.
    - `account::account_failures(&conn, 1)` is then 30.
    - The right password from `203.0.113.4` answers 204, and the count stays 30.
    - A wrong one from `203.0.113.5` answers 401, and the count is 31.
  - `a_wrong_basic_password_counts_against_its_account`:
    - A wrong Basic password from `203.0.113.1` answers 401, and the count is 1.
    - The right one from `203.0.113.2` answers 200, and the count stays 1.
  - `a_rename_keeps_the_accounts_count`:
    - A wrong login as `local` from `203.0.113.1`.
    - Rename the account, by `account::update`'s name patch or `UPDATE user SET name`.
    - A wrong login under the new name, from `203.0.113.2`.
    - The count is 2.
  - `an_unknown_name_counts_against_no_account`: a wrong login and a wrong Basic request for `nobody` leave exactly two `login_attempt` rows, their shared pair and address keys.
  - `a_wrong_current_password_counts_against_no_account`: a wrong password change leaves `account_failures(1)` at 0.
  - `a_refused_count_still_answers_401`:
    - A trigger refuses an insert under `account_key(1)`. Shape it like `a_claim_fault`: `BEFORE INSERT ON login_attempt WHEN NEW.key = '<account_key(1)>'`, then `RAISE(ABORT, …)`.
    - A wrong login answers 401, and so does a wrong Basic request.
    - The pair's count is 2.
  - `the_alarm_line_names_the_account`:
    - `alarm_line("sam", 2)` equals the literal sentence.
    - `alarm_line` of a name holding a quote and a newline returns a line with no newline, in which the name appears escaped.
  - **Two O57 tests change.**
    - `a_login_with_a_wrong_password_records_its_failure_holding_its_permit` and `a_basic_request_with_a_wrong_password_records_its_failure_holding_its_permit` now expect `[false, false, false]`: the pair's, the address's and the account's records, all made under the permit. Their docs say so.
    - The two unknown-name tests beside them stay at two.
  - Run both crates. The new tests fail, and so do the two changed ones.
- [ ] **Step 3: Implement.** Run both crates to green.
- [ ] **Step 4: Watched mutations.**
  - Write `== ALARM_AFTER` as `>=`: the 31st answers true, and the library test fails.
  - Make `ALARM_AFTER` `2 * LOCKOUT_AFTER`: the library test fails at the 20th.
  - Drop the window filter from `account_failures`: the lapse assertion fails.
  - Remove `count_wrong_password` from `login`: the thirty test fails.
  - Remove it from `from_basic`: the Basic test fails.
  - Read the alarm as a lockout, refusing before the verify at `ALARM_AFTER`: the 204 becomes 429.
  - Make a success clear `account_key`: the count after the 204 is 0.
  - Join the count's error into the `pair`/`address` decision: the refused test answers 429.
  - Call `count_wrong_password` twice: a commit test fails.
  - Call it on the unknown-name branch: the unknown-name test fails.
  - Call it in `change_password`: the current-password test fails.
  - Write `{name}` for `{name:?}`: the line test fails.
  - Key the count by the account's name: the rename test fails.
- [ ] **Step 5:** Run clippy and fmt. Commit `… (O279)` and write `$E/T4-report.md`.
  - The report names spec gap 1: the print itself is seen in the Finish's drive.

---

### Task 5: The count on the Users screen (O279, part 2)

Spec §4 "Where it shows", its second half.

**Files:**
- `crates/taiyaku-serve/src/users_api.rs`:
  - `user_json` (~104) takes `wrong_passwords: i64` and sends `"wrong_passwords"` and `"wrong_password_alarm"` (`wrong_passwords >= account::ALARM_AFTER`).
  - `assembled` (~156) reads `account::account_failures(conn, account.id)?`.
  - So every answer that renders an account carries it: the list, a create and a patch.
- `crates/taiyaku-serve/src/users_api/tests.rs`: a new test beside `the_user_list_carries_sessions_keys_and_grants` (~8).
- `web/src/users.ts`:
  - `User.wrong_passwords: number`. Its doc: the current window's count across every address, which refuses nothing.
  - `User.wrong_password_alarm: boolean`. Its doc: the server's verdict that the count reached its ceiling; the web holds no ceiling of its own.
- `web/src/UserTable.tsx`:
  - After the `locked` mark (~116-118), where `user.wrong_password_alarm`, add `<span className="mark">{JOINING_DOT}{user.wrong_passwords} wrong passwords in 15 minutes</span>`.
  - No CSS change: `.config-table .mark` already draws it.
- Every `User` fixture in `web/tests` gains `wrong_passwords: 0, wrong_password_alarm: false`. The files are those `grep -arl 'can_log_in:' web/tests` lists, browser tests among them.
- `web/tests/Users.test.tsx`: a new case beside 'marks the row of an account nobody can sign in as' (~272).
- `README.md`: one sentence after Task 2's paragraph.
  > When one account's wrong passwords reach 30 within 15 minutes, from however many addresses, taiyaku logs one warning naming it and the Users screen marks it; nothing is locked, and each address and name keeps its own allowance.

- [ ] **Step 1: Write the failing tests.**
  - Rust, `the_user_list_carries_each_accounts_wrong_passwords_in_this_window`:
    - Three `note_account_failure`s for account 1, and none for a second account (the file's `a_reader`).
    - The list sends 3 and 0.
    - With account 1's window started 16 minutes back, it sends 0.
  - jsdom, 'marks the row of an account whose wrong passwords reached the alarm':
    - Three accounts, none of them the signed-in account: sam at 30, kim at 29, and lee at 31 with `can_log_in: false`.
    - `td.lead`'s text, row by row:
      - sam: `'sam · 30 wrong passwords in 15 minutes'`.
      - kim: `'kim'`.
      - lee: `'lee · locked · 31 wrong passwords in 15 minutes'`.
  - Run the serve crate and the jsdom project. Both tests fail.
- [ ] **Step 2: Implement** the field, the type, the constant, the mark and the fixtures.
  - Run the serve crate, then the node, jsdom and whole browser projects.
  - Then `tsc -b` and `biome check`.
- [ ] **Step 3: Watched mutations.**
  - Read the field without the window: the Rust lapse assertion fails.
  - Send a constant 0: the Rust test fails.
  - Write `>=` as `>` in `user_json`: the Rust test's account at exactly 30 reads no alarm and fails.
  - Draw the mark whatever `wrong_password_alarm` says: kim's row fails.
  - Draw a literal 30 in place of the account's count: lee's row fails (its fixture count is not 30).
  - Put the mark before "locked": lee's row fails.
- [ ] **Step 4:** Write the README sentence. Commit `… (O279)` and write `$E/T5-report.md`.

---

### Task 6: The closes in `docs/status.md`

Read the Open items section, rows O277, O278 and O279, and the notes bullets of O266 and O272-O274 for their shape.

- [ ] **Step 1: Close O277, O278 and O279** with the five-part edit.
  1. Each row's checkbox.
  2. Each row's text, rewritten to say what was done, with symbols and no line numbers:
     - O277: the four headers on every answer, the theme file, and HSTS left to the proxy.
     - O278: `limiter_address`, and the session row still keeping the whole address.
     - O279:
       - It counts a wrong password at the login route and over Basic, not an unknown name or a password change.
       - It is logged once at 30 in a window and marked on Users.
       - It locks nothing.
  3. A notes bullet for each. None exists yet. Add `- **O27x — <the row's title>. Closed.** …` after O274's bullet, shaped like O266's.
  4. The "N are now closed — leaving M" sentence.
  5. The "What remains … M open items" line. Its count word wraps across a line break, so grep the opener and read the next line.
- [ ] **Step 2: Counts.** Measure them; do not add.
  - Rust:
    - Run `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml --workspace --all-features --test-threads=8 >| $E/T6-nextest.log 2>&1`.
    - Then `node $W/scripts/status-test-count.mjs $W/target/nextest/default/junit.xml $W/docs/status.md`.
    - Set both figures to what it reports, and run it again to see it pass.
  - Web:
    - Run the node and jsdom projects.
    - Set "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one" from the two summary lines.
    - The browser figure does not move, as no browser test is added. Say so in the report.
- [ ] **Step 3: Sweep.**
  - Check the sentences against `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`.
  - Grep each bare O-number (`O277`, `O278`, `O279`) across `docs/`, `web/src/`, `crates/` and `README.md`. Read every hit, and correct prose that speaks of one as open.
  - Grep the old behaviour's words and correct what they now get wrong:
    - `inline script`, `pre-paint script` and `Content-Security-Policy` in `web/src` and `docs/status.md`.
    - `rendered \`SocketAddr\`` and `digits, dots and colons` in `crates/`.
    - O272's notes bullet, where it lists what a wrong Basic password writes.
- [ ] **Step 4:** Commit `Close O277, O278, O279 in status.md` and write `$E/T6-report.md`.

---

### Finish

1. **Whole-branch review** of `BASE..HEAD`, in two lanes:
   - (a) The server: the Rust of Tasks 2-5, against spec §4, the anti-goals, O57 and O272.
   - (b) The web and the words: Task 1, Task 5's web, both README edits and Task 6.

   One fix wave goes back to the implementers by SendMessage. After a fix, grep for its shape, not its function. Review the fixes too.
2. **The CSP hand-check** (spec §9; a named step, run by a subagent). It runs before the final gate, since a policy source it finds is a code change.
   - **Setup.** `S=$E/drive`.
     - Check `cat /proc/loadavg`, and that `ss -lptn 'sport = :4402'` is empty. If it is not, take another free port and say so.
     - Write `sha256sum /home/davey/temp/manga-test/library.db` to `$E/drive-before.sha256`, and `touch $S/marker`.
     - Copy, never hard-link: `cp /home/davey/temp/manga-test/library.db $S/library.db`. Never open the original.
     - Build: `cargo build --release --manifest-path $W/Cargo.toml -p taiyaku-cli`, then copy `$W/target/release/taiyaku` to `$S/taiyaku`.
     - Set the password: `printf 'pw\npw\n' | $S/taiyaku library user password local --db $S/library.db`.
     - Build the app: `builtin cd $W/web && ./node_modules/.bin/vite build --outDir $S/dist --emptyOutDir`.
   - **Serve.**
     - Start `$S/taiyaku serve --db $S/library.db --cache $S/cache --web $S/dist --addr 127.0.0.1:4402 --insecure-cookies --trusted-proxy 127.0.0.1/32 >| $S/serve.log 2>&1` with `run_in_background`.
     - Never restart it detached. Stop it by PID.
     - Check that `curl -sI http://127.0.0.1:4402/` shows each of the four headers exactly once.
   - **The script** is `$E/csp-check.mjs`.
     - Run it with `node`. It imports `chromium` from `$W/web/node_modules/playwright/index.mjs` by absolute path; it does not use the chrome-devtools MCP.
     - Headless, one context with `colorScheme: 'light'` and `acceptDownloads: true`, at 1280 px.
     - It records, per phase, into `$E/csp-check.json`:
       - each `securitypolicyviolation` event, from a listener an init script adds;
       - each console error or warning;
       - each `pageerror`;
       - each same-origin answer ≥ 400, except the anonymous `/api/me` 401 before sign-in.
     - It saves screenshots to `$E/csp-<phase>.png`.
   - **Phases:**
     1. **Pre-paint, "Ink on a light OS paints dark first".**
        - Store `{"choice":"ink"}` under `taiyaku.prefs.theme`, block `/assets/*.js`, and load `/`.
        - Expect `<html data-theme="ink">`, with the page background ink's `#15130f` (`index.html`'s dark `theme-color`).
        - Control: block `/theme.js` as well. Expect no `data-theme`, and paper's `#f3eee3`.
     2. **Sign-in:** load `/`, and sign in as `local`/`pw` through the form.
     3. **About** (`/about`) and **the licence page** (`/about/licence`).
     4. **The reader**, at `/read/<slug>` for a work `GET /api/works` lists.
        - A page is drawn, and `/assets/*.wasm` answers 200.
        - Turn one page.
        - Add a note and press Export: expect a `download` event with a `.json` name.
     5. **The labeller:** load `/label.html`, pick a work, and see its page drawn.
     6. **Configuration and Users.**
        - Open a library's page and one of its directories' pages.
        - Press Export in the patterns rule editor: expect a `download` event.
        - Open `/config/users`.
     7. **Theme:** choose Ink in the sidebar's control, and reload.
     8. **Control.** The script injects a `<script>` with an inline body into the page. Exactly one violation must be recorded, or the zeros before it are not believed.
   - **Pass:**
     - Phases 1-7 show zero violations and no console line naming the Content Security Policy.
     - The control records exactly one violation.
     - Both downloads fire, the wasm answers 200, and both pre-paint readings hold.
     - Where the test library cannot reach a phase, the report says how that phase was covered instead. The reader and the labeller are not optional.
   - **A violation is a finding.** Fix it by adding the source the browser names to the policy (spec §8 allows this), and update the two tests' literal. The PR names the source and what needed it, and the review re-reads that fix.
3. **The drive** (spec §9), by `curl` against the same server through the trusted peer, with `X-Forwarded-For` naming the client.
   - **/64**, for the unknown name `nobody`, so that `local`'s count stays clean:
     - five wrong passwords from `2001:db8:1:2::1` and five from `2001:db8:1:2::99` answer 401;
     - one from `2001:db8:1:2::abc` answers 429;
     - one from `2001:db8:1:3::1` answers 401;
     - `sqlite3 -readonly $S/library.db "SELECT key, count FROM login_attempt"` shows the `2001:db8:1:2::/64` keys.
   - **The alarm**, for `local`:
     - Ten wrong passwords from each of `203.0.113.1`, `.2` and `.3` answer 401. `$S/serve.log` then holds exactly one line `WARNING: 30 wrong passwords for the account "local" (id <its id>) in 15 minutes. …`.
     - A 31st wrong password, from `.4`, answers 401 and adds no line.
     - The right password from `.5` answers 204.
     - `GET /api/library/users` sends `local`'s `wrong_passwords` as 31.
     - The script's phase 6 is run again: Users shows the mark at 1280 and at 390 px. Look at both screenshots, and the console stays clean.
4. **Stop and prove.**
   - Stop the server by PID, and check that `ss -lptn 'sport = :4402'` is empty again.
   - Check that `find /home/davey/temp/manga-test -newer $S/marker` prints nothing, and that the `library.db` sha256 equals `$E/drive-before.sha256`.
   - After the controller has read the reports, delete `$S` by its absolute path, in a call of its own.
5. **Final gate** on the final commit, at load < 24. Read the `.exit` file.
6. **Claim check** of the closes, both README edits and the PR body.
   - Pass 1 includes the PR body.
   - Later passes are scoped to the correction commit plus the PR body.
   - Grep each corrected sentence's shape everywhere.
7. **The PR body** goes in `$E/pr-body.md`, with the title as its first line, measured under 255 characters. It names any policy source the hand-check added. Ask the owner before pushing; the forge is Gitea, through its MCP.
8. **After the merge**, once WUD has redeployed it (spec §9):
   - `curl -sI https://<public name>/` shows each of the four headers exactly once, so neither NPM nor Cloudflare doubles one.
   - The owner opens the public name with the console open, and checks the sign-in, the reader with a page turn, and About: no line naming the Content Security Policy.
   - With the owner's go-ahead, the script's anonymous phases (1, and the sign-in screen) may run against the public name too.

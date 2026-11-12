# Going public: four lanes

**Date:** 2026-11-12. **Status:** the decisions are the owner's rulings of
2026-11-12, the builder's calls told to the owner that day, which the owner
may overrule, and the spec's own calls where both are silent. Each decision
says which it is: "ruled", "the builder's call" or "the spec's own call".
Each lane's plan is written from this document.

The rulings and surveys behind it are in `.superpowers/sdd/deploy-evidence/`,
local and not committed; what a plan needs from them is restated here. Line
numbers are at `main` ec4eb11c. "(inferred)" marks what was reasoned, not
read or run.

## 1. Why, and the lanes

taiyaku is going into daily use on the owner's Unraid NAS, public through
Cloudflare and Nginx Proxy Manager (NPM), and other people will get accounts
on it later. The pre-deploy lanes have merged (#233 to #238), but for lane
4's second PR, and WUD redeploys the image on every merge to `main` (ruled
2026-11-12: that stays), so what merges is what runs. Three things make this
the time:

- **The real library has never been scanned (the repo report's B1), and an
  added directory is scanned at once.** The add inserts the row with no interval (`scan.rs:215`),
  which reads as Automatic, and a directory never scanned is due
  (`schedule.rs:88-92`) at the next 30-second tick (`scheduler.rs:18`,
  `:89`). So an edition key, patterns or Off cannot be set first, a NAS step
  the pre-deploy spec lists (`2026-11-10-pre-deploy-design.md:1292-1294`).
- **Going public puts the sign-in on the internet.** No security header is
  sent (none in `crates/taiyaku-serve/src` or `web/index.html`, by grep), the
  limiter keys an IPv6 client by its whole address (`session.rs:491`, `:496`;
  `auth.rs:726`, `:733`), and nothing counts one account's failures across
  addresses (`account.rs:602`, `:646`).
- **A second account meets open rows**: O58, O267, O241, O240, O184 and
  O248, each in lane 3 or 4.

The repo report's `X-Real-IP` spoof was tested false on the Cloudflare path
on 2026-11-12, as Cloudflare drops a visitor's `X-Real-IP`; the real gap,
visitors keyed by shared edge addresses, was fixed in NPM, not in code.

| Lane | What | Rows | Size |
| --- | --- | --- | --- |
| 1. Add as Off and the README | a schedule chosen at the add; README D1–D4 | closes O275, O276 | M |
| 2. Server hardening | security headers; the /64 key; the per-account alarm | closes O277, O278, O279 | M |
| 3. Second accounts | O58, O267 and its path sweep, O184, O248 | closes O58, O241, O267, O184, O248 | L |
| 4. Anchors on the wire | the web sends anchors; the removal's count | closes O240, O244 | L |

## 2. Goals, non-goals and anti-goals

**Goals.** The owner sets each real directory's edition key and patterns
before its first scan, and the README's steps work as written (lane 1). A
browser runs only the server's own scripts, styles and fonts, no other site
frames the app, a /64 spends one allowance, and the owner learns when one
account's failures pass a ceiling (lane 2). Only an administrator in a
browser writes what every reader is served, no reader sees a server path, no
write brings a deleted work back, and Move and Remove name everyone who
loses sight (lane 3). A note or position lands on the page it was written on,
whatever renumbering came after the tab read it (lane 4).

**Non-goals.** Locking or unlocking an account; HSTS from the server, or a
report-only policy; the interim O240 fix; a per-account capability to edit
shared documents (O58's option C); a CLI `library add`; anchors for
`page_flag`, `run` and `truth_page`; O214, O215, O270, O271, O273, O274,
O272's remainder, O51's watcher, and the pre-deploy lane 4's PR 2 (O91) but
for O184, which moves into lane 3; and NAS steps (NPM, Cloudflare, the
stack's labels, mounts, backups).

**Anti-goals: closed rulings no lane reverses.**

- **Auth spec §7** (`2026-08-14-library-auth-design.md:283-292`): the lockout
  is keyed on (client address, username), never on the account.
- **O57** (`docs/status.md:1665`): four verify permits, the lockout read
  before the wait and again holding one. Lane 2 adds no read before a verify.
- **O29** (`status.md:1637`): `atomic::write`'s errors name the path they were
  handed, for the CLI. Lane 3 maps the error at the route.
- **O86 and O93** (`status.md:1694`, `:1701`): an administrator is
  unrestricted (`Access::Every`, `auth.rs:319`, `:408`), and
  `session::administering_or_forbidden` (`session.rs:858`) alone answers "may
  this caller administer".
- **O43** (`status.md:1651`): a correction writes the file, then the rows,
  then memory; O184 resolves the row first in the same transaction (inferred:
  the order holds).
- **Overlapping-libraries decisions 8 and 11**
  (`2026-11-02-overlapping-libraries-design.md:59-62`, `:66-72`): a narrowed
  edition's truth write and correction are refused, and the `ETag` names the
  account's numbering, with no reload on a 412.
- **Pre-deploy lane 3 decisions 1 to 3**
  (`2026-11-10-pre-deploy-design.md:646-656`): one schedule per directory,
  shared by its libraries; Automatic NULL, Off 0, Every at least 5 minutes.
- **Pre-deploy lane 2 decisions 2 and 7** (`:377-381`, `:399-403`): a lost
  file's note keeps its stored slot and is `stale`, the row is never
  rewritten, and exported documents carry no anchor.
- **O263** (`status.md:1872`): the README restores by a move, which
  `schema/tests.rs:750-752` rests on.

## 3. Lane 1: Add as Off, and the README

### Decisions

1. **Build Add as Off** (ruled 2026-11-12). The add form gets a box, "Don't
   scan yet (schedule Off)", which starts unticked.
2. **The field is `scan_interval_secs` on both add `POST`s, in the `PATCH`'s
   vocabulary** (the builder's call): absent or null Automatic, 0 Off, 300 or
   more Every; 1 to 299 or a negative refused in the `PATCH`'s sentence.
3. **A directory another library already holds keeps its shared schedule, and
   the answer says so** (the builder's call). Setting Off for every holder
   would let one library silence another's scans (inferred).
4. **It stays Off after the first Scan press** (the builder's call).
5. **`library scan --add` is unchanged** (the builder's call): it adds and
   scans at once by design (`taiyaku-cli/src/library.rs:320`, `:358`).
6. **The interval goes into the add's own `INSERT`** (the spec's own call):
   an add and then a `PATCH` can lose to a tick between them (inferred).
7. **The README fixes D1–D4 ride here** (ruled 2026-11-12 as a docs PR; the
   spec's own call to carry them in this lane, as D4 describes decision 1).

### Where

- `scan::insert_root` (`scan.rs:199-215`) writes the interval in its
  `INSERT`; `add_root_in` (`:189`), `add_root_checked_in` (`:504`) and
  `create_library` (`:854`) pass it down; a link (`Placed::Linked`, `:566`,
  `:928`) writes no schedule.
- The check in `config_api::root_patch_from_json` (`config_api.rs:1305-1316`)
  becomes one function the two `POST`s and the `PATCH` share.
- `libraries_api::new_library_from_json` (`libraries_api.rs:312`) and
  `dir_path_from_json` (`:447`) read the field; `create_library` (`:335`) and
  `add_dir` (`:459`) pass it on (routes `routes.rs:2254`, `:2263`). A create
  applies one value to each directory it adds.
- `config_api::added_json` (`config_api.rs:1256`) already answers the stored
  `scan_interval_secs` and `also_in`; a link whose stored schedule differs from
  the one asked for also says so in a field of its own.
- `NewLibrary.tsx` (`:117`) and Add directory… (`LibraryPage.tsx:1154`) draw
  the box, through `libraries.ts`' `createLibrary` (`:342`) and `addDir`
  (`:357`): ticked sends 0, unticked no field. A kept shared schedule is said,
  naming the libraries. An Off directory never scanned says so beside "Never"
  (`config.ts:449`, `:463`, `:474`; `LibraryPage.tsx:1408`).
- **D1.** `README.md:154-155` set `Host` and `X-Forwarded-Host` to `$host`;
  they become `$http_host`, as at `:623-624` for the reason at `:638-640`.
- **D2.** The restore (`:226`) and roll-back paragraph (`:677-693`): stop the
  container; move `library.db`, `library.db-wal` and `library.db-shm` aside;
  move the backup in as `library.db`; `chown 99:100` it; start. They name the
  host directory under the compose mapping (`:490`, `:507`). A stale `-wal`
  beside a restored file can be applied over it (inferred).
- **D3.** The compose block (`:497-510`) gets the WUD labels named at `:669`.
- **D4.** The setup text (`:543-548`, `:557-566`) says an added directory is
  scanned within one tick, about 30 seconds, unless "Don't scan yet" is
  ticked, which is how to set an edition key or patterns first.

Closes O275 (Add as Off) and O276 (the README). Size M.

## 4. Lane 2: server hardening

### Decisions

1. **One PR** (ruled 2026-11-12): security headers; the IPv6 key at /64,
   canonicalising a mapped IPv4 address first; a per-account alarm that
   counts, logs and shows past a ceiling and locks nothing (auth spec §7's
   pair lockout stays).
2. **The theme script moves to a file of its own**, so the policy allows no
   inline script (the builder's call).
3. **Framing is denied**, by `X-Frame-Options: DENY` and `frame-ancestors
   'none'` (the builder's call).
4. **HSTS stays with NPM and Cloudflare** (the builder's call): NPM already
   sends it, and the proxy holds the TLS.
5. **One layer puts the headers on every answer, enforced from the start**
   (the spec's own call). OPDS clients are not browsers (inferred), and with
   no report endpoint §9's hand-check is what report-only would buy.
6. **The alarm's ceiling is 30 failures in one 15-minute window, logged once
   and shown on Users** (the spec's own call; the owner may change it).

### Security headers

- An `axum::middleware::from_fn` on `app` in `routes::build_router_serving`
  (`routes.rs:3016`), after the merge and the `--web` fallback
  (`:3068-3076`), inside the log layer (`:3078`); no new `tower-http` feature
  (`crates/taiyaku-serve/Cargo.toml:45`).
- `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`,
  `X-Frame-Options: DENY`, and `Content-Security-Policy: default-src 'self';
  script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; font-src 'self'
  data:; img-src 'self'; connect-src 'self'; object-src 'none'; base-uri
  'none'; form-action 'self'; frame-ancestors 'none'`.
- Why: the reader instantiates `/assets/*.wasm` from a `--target web` build
  (`scripts/build-wasm.sh:19`), which needs `'wasm-unsafe-eval'` (inferred);
  a 2026-10-20 build's CSS inlines the mark fonts (`tokens.css:62-78`) as
  `data:` URLs; React's `style` props go through the CSSOM, which `style-src`
  does not block (inferred); the notes export is a `blob:` link
  (`NotePanel.tsx:141`), a navigation and not a fetch (inferred).
- The theme script (`web/index.html:20-29`) and its comment (`:9-19`) move to
  a file under `web/public/`, which Vite copies unhashed and `web.rs` serves
  with `no-cache` (`web.rs:35`); a classic `<script src>` in `<head>` still
  runs before first paint (inferred). `web/tests/theme.test.ts` (`:67-83`)
  reads the file.
- The README says the server sends these headers and HSTS is the proxy's.

### The /64 key

- One function writes the address half of every key (the spec's own call):
  canonical first (`IpAddr::to_canonical`, so `::ffff:a.b.c.d` is
  `a.b.c.d`), then an IPv4 address whole and an IPv6 address as its /64.
  Without the first step every mapped IPv4 client is in `::/64`, one key for
  all (inferred).
- Its callers: login's pair and address keys (`session.rs:491`, `:496`),
  Basic's (`auth.rs:726`, `:733`), and the password change's
  (`session.rs:1102-1103`). The session row keeps the full address
  (`session.rs:583`); `auth::client_address` (`auth.rs:142-154`) is unchanged.
- No migration: a key is text (`schema/004_auth.sql:121`), and old-shape
  rows lapse with their window (`account.rs:586`). The address ceiling's doc
  (`:609-610`) adds that a household on one /64 is one address.

### The per-account alarm

- **What counts:** a wrong password for a name that names an account, at
  `session::login` (`session.rs:465`) and `auth::from_basic` (`auth.rs:691`).
  Not an unknown name, which names no account and which the address key
  bounds, and not the password change, whose caller is signed in.
- **The key** is the account's id, so a rename keeps the count, and it must
  not collide with the other shapes (the rule in `account.rs:597-600` and
  `:642-644`). Window and table are `note_failure`'s (`:655-680`).
- **Why 30:** three pairs' allowance (`LOCKOUT_AFTER`, `account.rs:587`). A
  pair records about ten failures, plus the few O57's permits let past, so 30
  needs three sources or more (inferred); a guesser held under 30 a window is
  not seen, and a longer window is the overrule.
- **It refuses nothing and is never read before a verify**, so O57's bound
  holds and O215's list of reads that can fail open does not grow (inferred).
  It is written after the two limiter writes, once and without waiting
  (`taiyaku_library::without_waiting`, as at `auth.rs:814`), and a refused
  write is dropped, so a failure during a derive's lock goes uncounted. A
  success does not clear it; it lapses with its window, as the address key
  does.
- **Where it shows:** one stderr line naming the account when
  `note_failure`'s count reaches the ceiling, once per window; and the
  window's count in `users_api::user_json` (`users_api.rs:104`), which
  `UserTable.tsx` marks at or past the ceiling beside "locked" (`:116-118`).

Closes O277 (headers), O278 (the /64) and O279 (the alarm). Size M.

## 5. Lane 3: second accounts

### Decisions

1. **O58: the shared truth and alignment documents are writable only by an
   administrator signed in through the browser; keys may read them** (ruled
   2026-11-12). It closes O241 by consequence.
2. **O267 with the path sweep** (the builder's call): the page, thumbnail and
   download 404s stop naming a server path. `patch_alignments`' 500 keeps its
   path, because O58 makes it an administrator's route and the O267 rule below
   lets an administrator's routes keep theirs (the spec's own call); O267
   closes by O58 plus the sweep. `atomic::write` keeps naming its own (O29).
3. **O248 asks about the moved directory and every directory nested in it**
   (the builder's call).
4. **O184 as the pre-deploy plan's Part 2 Task 4, after O58** (the builder's
   call), since both change `patch_alignments`' lines.
5. **The gate is `administering_or_forbidden`** (the spec's own call, by O93),
   so a key and another role get its existing refusals (`session.rs:827`,
   `:863`), which stay true once these writes are an administrator's.
6. **Overlapping-libraries decision 8's write halves and `truth::as_stored`
   stay**, unreachable while only an administrator, always `Every`, writes
   (the spec's own call; a later per-account capability would reach them,
   inferred).
7. **The web offers correcting and labelling only to an administrator**, from
   `/api/me`'s `admin` (`session.rs:1192`), and **a path-free answer logs the
   full error to stderr** (the spec's own calls).

### Where

- **O58.** `put_truth` (`routes.rs:1350`) and `patch_alignments` (`:1762`)
  open with the gate; `auth/tests.rs:3978` declares both, `:3883` stays
  green, the route tables (`routes.rs:2408`, `:2426`, `:2893`) are read again,
  and the doc calling them "deliberately untouched" (`session.rs:852-857`) is
  rewritten. `CORRECTION_PORTS` (`App.tsx:248`) are wired for an
  administrator only; `label.tsx` and `Labeller.tsx` show others a sentence.
- **O241** closes: an administrator is `Access::Every` (`auth.rs:319`,
  `:408`), and `truth::as_stored` returns the document whole for a view that
  is not narrowed (`crates/taiyaku-serve/src/truth.rs:127-128`).
- **O267.** The rule: a body a non-administrator can be sent never carries a
  server path; an administrator's routes, such as the stamp's refusal, keep
  theirs. `patch_alignments` sends `atomic::write`'s error as its 500 body
  (`routes.rs:1996-1997`) and keeps doing so, an administrator's route once
  O58 lands. `get_page` and `get_thumb` answer `not_found(e)`
  (`:819`, `:950`), where `e` carries `Work::path` (serve `lib.rs:343`) and a
  source message naming the member's path (`taiyaku-read/src/pages.rs:454`,
  `:455`, `:470`, `:473`, `:484`). The downloads 404 with `{path}: {e}`
  (`download.rs:712`, `:1281`). Each sends a sentence naming the work, and
  page where there is one. The builder sweeps the reader routes for any
  other body built from `.display()` or a source's error, and lists the finds
  in the PR.
- **O184**, as the pre-deploy spec writes it
  (`2026-11-10-pre-deploy-design.md:1058-1065`), and its plan:
  `works::row_id_for_write` (`works.rs:425`) and `row_id_for_document`
  (`:448`) mint no `bridge:` row for a live registry (`Works::is_live`,
  `registry.rs:186`), so the four writers answer 404 `` no work `{slug}` ``,
  and `patch_alignments` resolves the row before its file write; a fixed
  registry still mints (`alignments.rs:38-39`, serve `lib.rs:90-94`). The
  pre-deploy lane 4's PR 2 drops the task, its decision 5 (`:911`) done here.
- **O248.** `relocate::sighted` (`relocate.rs:191`) and `leaving` (`:227`)
  compare each limited account's sight of the moved or unlinked directory and
  of every directory row nested in it, before and after. An account that loses
  any is in `loses_sight`, one that gains any in `gains_sight`, and one can be
  in both (the spec's own call). `scan::Unlinked` (`scan.rs:1095`) takes the
  same answer. `MoveCounts.tsx` (`:20-27`) and `UnlinkRoot.tsx` (`:157-160`)
  say "it or a directory inside it", as does the overlapping-libraries
  design's §3.5 (`2026-11-02-overlapping-libraries-design.md:406-408`).

Closes O58, O241, O267, O184 and O248. Size L: O58 M with its web half, O267
and the sweep M, O184 S (its task is written), O248 S.

## 6. Lane 4: anchors on the wire (O240, O244)

### Decisions

1. **The web sends anchors** (ruled 2026-11-10,
   `2026-11-10-pre-deploy-design.md:391-392`), **as the full fix, now**
   (ruled 2026-11-12, over a stopgap). It closes O240 and O244's remainder.
2. **No reload on a 412** (ruled, as overlapping-libraries decision 11). `saveNotes`'
   re-read and `mergeOver(fresh, mine)` (`notesWrite.ts:161`, `:191`) keep
   their shape.
3. **The wire's anchor is `at`, optional on each note and on a position**
   (the spec's own call): `sha256`, `nth`, `page` and `half`, as `MemberPage`
   (`taiyaku-core/src/sidecar.rs:345-353`), `half` absent for a whole page. It
   is in the account's numbering, `nth` counting the members the account is
   sent, and the server translates it at its edge as it does a slot
   (overlapping-libraries decision 7, `:56-58`).
4. **The server answers `at` on its `GET`s**, so the web echoes what it read
   (the spec's own call). Today, after a renumbering, the retry re-sends every
   note the tab holds (`App.tsx:1648`), and an untouched one at its old slot
   counts as moved and is re-anchored on what that slot shows now
   (`state.rs:717-794`; `Note::same_anchor`, `taiyaku-library/src/lib.rs:195`;
   inferred).
5. **A valid anchor wins over the slot** (the spec's own call, from the
   pre-deploy spec's `:554-556`). An `at` that resolves in the layout the
   server numbered the account's document with sets the slot. A note whose
   `at` equals its stored anchor keeps it, resolving or not. Any other `at`
   naming no member the account is sent is refused in `notes::Invalid`'s
   words (`notes.rs:46`). A body without `at` is read by its slot. A `GET`
   sends a stale note's stored anchor as stored, and a write's `at` is
   compared with the stored anchor for equality before any translation into
   the account's numbering (the spec's own call), so a narrowed edition's
   stale note round-trips unchanged.
6. **The web computes anchors through `taiyaku-core`** (the spec's own call):
   `Edition::member_page` and `slot_of` (`sidecar.rs:287`, `:313`) exported
   beside `WasmSidecar::slot_at` (`taiyaku-wasm/src/lib.rs:138`), not copied
   into TypeScript.
7. **Exported documents stay anchor-free, and `taiyaku_notes` stays 1** (the
   spec's own call, keeping the pre-deploy decision 7): `at` is a wire field
   like `stale`, cleared on export and ignored by `library import`. `Note` is
   `deny_unknown_fields` (pre-deploy spec `:527-529`), so an older binary
   would refuse a document carrying it (inferred).
8. **`stale` is drawn**, as a note no longer on its page; an edit keeps its
   anchor and a move re-anchors it (the spec's own call).
9. **The removal rehearsal counts the rows it would leave unresolved** (ruled:
   the pre-deploy decision 8, `:404-406`, deferred it here).

### Where

- Server: `get_notes` (`routes.rs:1042`), `put_notes` (`:1118`),
  `get_progress` (`:1445`), `put_progress` (`:1508`); serve `notes.rs`
  (`validate_against`, `:100`; `as_stored`, `:260`) and `progress.rs`; library
  `state::put_notes_in` (`state.rs:717`) and `put_progress_at` (`:222`). The
  library's `Note` (`lib.rs:154`) and the position gain `at`, resolved against
  `View::full`, the layout the reader's document was numbered with (O244's
  row, `status.md:1853`). The notes `PUT` reads `at` to place a note, then
  clears it with `stale` before `notes::as_stored` compares whole notes
  (inferred: an echo would otherwise read as a change).
- Web: `notes.ts`' `Note` and `api.ts` carry `at`. A created or moved note
  takes `at` from the layout the reader draws; an untouched note keeps the
  `at` it was read with; `editsToWrite` (`notes.ts:687`) and `mergeOver`
  (`:706`) carry it, so the 412 retry sends each note's anchor. Each progress
  write sends `at` for the page shown. `NotePanel.tsx` draws `stale`.
- Count: `scan::removal_cost` (`scan.rs:1969`) and the library removal's
  dry run `library_removal_cost` (`libraries_api.rs:554`) count the positions
  and notes on works the removal keeps whose anchor resolves before it and not
  after; `RemoveRoot.tsx` and `RemoveLibrary.tsx` show it. An unlink derives
  nothing and moves no row (`scan.rs:1128-1130`, `UnlinkRoot.tsx:12-14`), so
  it has no such count and `UnlinkRoot.tsx` is not in this lane.
  It is the dry run's; the real removal's answer stays as O271 records it.

Closes O240 and O244. Size L; the builder may ship it as two PRs, the wire
and then the count.

## 7. Overlaps and the merge order

**Merge order: 1, 2, 3, 4.** Lane 1 lands before the owner adds the real
directories, lane 2 before the public name opens, lane 3 before a second
account exists, and lane 4 after lane 3, whose O184 changes the same writers.
Lanes 1 to 3 can be built at once in worktrees of their own; lane 4's server
half waits for lane 3 on `main`. Within lane 3, O58 (the gate), then O267 and
O184 (the write branch, `routes.rs:1979-2000`) edit `patch_alignments` in
that order; O248 is independent.

**Files two lanes share**, mostly in different functions. `routes.rs`: lane 2
(`build_router_serving`), lane 3 (`put_truth`, `patch_alignments`,
`get_page`, `get_thumb`, O184's four writers) and lane 4 (`get_notes`,
`put_notes`, `get_progress`, `put_progress`), so lanes 3 and 4 meet in
`put_notes`, `put_progress`, and serve `notes.rs` and `progress.rs`.
`config_api.rs`: lane 1 (`root_patch_from_json`, `added_json`) and lane 4
(`removal_json`, which destructures `scan::Removal` without `..`, `:1335`).
`web/src/config.ts`: lanes 1 and 4 (inferred).
`session.rs`, `auth.rs` and `auth/tests.rs`: lanes 2 and 3. Library
`scan.rs`: lanes 1, 3 and 4. `App.tsx`: lanes 3 and 4.
`README.md`: lanes 1 and 2. `docs/status.md`: every lane, in its last task,
redoing the counts (the total heading Open items, its "leaving N", and What
remains) when it rebases onto a lane that moved them, and the test count
`scripts/status-test-count.mjs` checks (`scripts/check-all.sh:282`).

## 8. What is left to the builder

Names (the add's "schedule kept" field, the alarm's key shape, the Users
count's field, the theme file, the home of the limiter's address function);
every sentence a person reads; a `from_fn` or a `tower-http` header layer; a
policy source beyond §4's that the hand-check finds, which the PR names with
what needed it; one PR or two for lane 4; and the README's wording.

## 9. How each lane is proved

Every lane: the repo's own `scripts/check-all.sh`, whole, never piped through
`tail`; each new test watched failing against its defect applied by hand; a
whole-branch review and a claim check of the PR body. A browser hand-check
where it touches the policy or the UI, and a drive on a live server over a
scratch copy of the test library where its behaviour is visible.

Lane 2 also checks the policy through the public name once WUD has
redeployed the merge: the reader, a sign-in and About open with a clean
console (the spec's own call). Cloudflare injected nothing into `/`'s HTML
through the public name on 2026-11-12 (no Rocket Loader, Web Analytics or
challenge script), so §4's sources are expected to suffice there.

**Lane 1.** `libraries_api/tests.rs` at `:415` and `:695`: an add with 0
stores 0, and `schedule::scheduled` (`schedule.rs:72`) leaves it out straight
after (*fails on:* the field ignored). Both `POST`s refuse 299 and -1 in the
`PATCH`'s sentence and store null and 300 (*fails on:* a check of their own).
A link asked for Off keeps the stored schedule and says so (*fails on:* the
shared row rewritten). jsdom (`Libraries.test.tsx`, `LibraryPage.test.tsx`,
`libraries.test.ts`): the box starts unticked, ticked sends 0, unticked no
field. Drive: add a directory ticked, see no scan past two ticks, set its
edition key, press Scan, see it scan and stay Off; add one unticked and see
it scanned within about 30 seconds. The claim check reads each changed README
sentence against the code.

**Lane 2.** `web/tests.rs` (`built_app`, `:21`; `get`, `:41`): `index.html`,
an asset, the theme file and an unknown path carry the four headers, as do an
`/api` and an `/opds` answer in `routes/tests.rs` (*fails on:* the layer on
the guarded routes only); `index.html` holds no inline `<script>`. The /64:
two addresses in one /64 share a pair and an address count, two /64s do not,
a mapped IPv4 address and its plain form share a key, and two different
mapped ones do not (*fails on:* the canonical step or the mask dropped). The
alarm: 30 wrong passwords for one account from three addresses print one
line and mark the account, 29 do not, a 31st prints nothing more, the right
password from a fourth signs in, an unknown name counts nothing, and a
refused alarm write still answers 401 (*fails on:* the alarm read as a
lockout). `account/tests.rs:402` and `session/tests.rs:581` pass unchanged.
Hand-check with chrome-devtools on a release build serving the built app
(vitest's browser project runs Vite's dev server, inferred): the reader with
a notes export, the labeller, About, Configuration and Users raise no policy
violation, and Ink on a light OS paints dark first. Drive: wrong passwords by
`curl` through a trusted peer, `X-Forwarded-For` naming three addresses.

**Lane 3.** O58: a key's truth `PUT` and alignments `PATCH`, and a `user`
session's, answer 403; an administrator's session writes; a key's `GET`s
answer 200 (*fails on:* either gate removed); jsdom: no editor for a `user`.
O267: an unwritable `taiyaku.json`, an unreadable archive's page and
thumbnail, and a deleted file's download answer with no part of the scratch
path, which stderr holds (*fails on:* the error passed through). O184: the
plan's tests (*fails on:* the mint restored). O248: the row's three-level
example (`status.md:1857`) names the account for the move and the unlink
(*fails on:* only the moved directory asked). Drive: a second account, role
`user`, with a key, meets no correction, a labeller that says why, and a
garbage archive's 404 with no path; Move on the three levels names it.

**Lane 4.** A note `PUT` with an old-numbering slot and a valid `at` lands on
the anchored page after a removal renumbers the edition (*fails on:* the slot
preferred); a hidden member's `at` is refused; an echoed unresolvable stored
anchor stays `stale`; a position `PUT` follows the same rules; a body with
no `at` keeps its slot. Export writes no `at`. vitest: the 412 retry sends
each note's `at`, and a new note's is the core's. `removal_cost` counts a
kept work's rows on a removed member only. Drive: a reader open while
another tab's removal renumbers its edition adds a note and turns a page,
and after a reload both are where they were made; the dialog showed the
count, and a stale note is drawn.

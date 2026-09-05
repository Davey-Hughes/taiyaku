# Building the library's configuration screens — design

**Status: settled in brainstorming on 2026-09-04. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is the **build design for spec C4's UI half**. The product design is
[`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
and the server half's build design is
[`2026-09-02-library-configuration-build-design.md`](2026-09-02-library-configuration-build-design.md);
neither is restated here. Those two settled *what* the four screens are and
*what answers them*. This one settles **what gets drawn, in what order, and
the one route that turned out to be missing.**

## 0. What exists, and the one thing that does not

The server half is complete and shipped — #105 (rules, roots, scan), #106
(users, backup), #107 (O93's credential gate). Every route the four screens
need is registered, admin-gated through
`session::administering_or_forbidden`, and covered by tests:

| Screen | Routes |
| --- | --- |
| Rule editor | `GET/POST /api/library/rules`, `PATCH/DELETE .../rules/{id}`, `POST .../rules/preview`, `POST .../rules/import` |
| Roots and scan | `GET/POST /api/library/roots`, `PATCH/DELETE .../roots/{id}` (`?dry_run=`), `GET/POST .../roots/{id}/scan` |
| Users | `GET/POST /api/library/users`, `PATCH/DELETE .../users/{id}` (`?dry_run=`), `DELETE .../users/{id}/sessions/{sid}`, `DELETE .../users/{id}/keys/{kid}` |
| Backup | `GET/POST /api/library/backups`, `GET .../backups/{name}` |

The web side has the chrome C1 built — `Shell.tsx`'s sidebar and `<main>`,
`Routes.tsx`'s five routes, `api.ts`'s `getJson`/`sendJson` with their one
place that routes a 401 — and **no settings area at all**. It also has three
names already taken by reader-side screens that are not this: `Settings.tsx`
is the reader's module panel, `BrowseSettings.tsx` is the sidebar's browse
preferences, and `Account.tsx` is the reader's own account overlay, mounted
from `App.tsx` rather than routed.

### Nothing applies a rule, and C4 §1's Save button promises it does

C4 §1 states the button verbatim: *"Save — apply to 71 works."* **No route in
this product applies a rule to anything.**

- `POST`, `PATCH` and `DELETE /api/library/rules[/{id}]` write the `rule`
  table and return. None of them calls `derive`. `config_api.rs`'s own
  `patch_rule` doc says so in as many words — *"nothing is server-derived
  about a patch"* — and `grep 'derive::' config_api.rs` finds it only on the
  scan path and the root-removal path.
- So a person who edits a rule through this screen changes the rule table and
  **nothing else**. The library's series, works, editions and members stay
  exactly as they were until somebody scans a root, and nothing on screen
  would say so.

The obvious substitute makes it worse. `POST /api/library/rules/import`
replaces the whole set in one transaction — which is the right shape — but
`rules::import_json` passes `id: 0` for every rule it writes, because an
exported document carries no ids of *this* library's. `replace_all` reads `0`
as "mint a fresh one", so every rule is a new row, `proposal.created_rule_id
REFERENCES rule(id) ON DELETE SET NULL` fires for every accepted proposal, and
**every save reopens every question a person has already answered.** The route
knows: it reports `reopened_proposals` precisely so that is not silent.

**The function that does this correctly already exists and has no caller that
commits.** `rules::replace_all` is a differential, id-preserving, atomic
update whose own doc comment names *"C4's editor"* and *"a real save"* — it
deletes only the ids genuinely dropped from the proposed list and updates
every kept row in place, so a save of an unchanged list reopens nothing. It is
`pub(crate)`, and its only non-test caller is `preview::preview`, inside a
transaction that is rolled back.

§4 is what closes this, and it is the one piece of Rust in these three PRs.

## 1. Scope: three PRs, and what is out

Decided with the owner, 2026-09-04, on C1's pattern — #81 the browse API, #82
the shell, #83 the pages.

**PR A — the settings area, roots and scan control.** C4 §9 steps 4 and 5.
The new chrome is proved on the smaller of the two structural screens, and
that screen carries the destructive action (`DELETE .../roots/{id}`, which PR
1's own whole-branch review calls the most destructive in the product) and the
live-polling one.

**PR B — the rule editor.** C4 §9 steps 2 and 3, plus the part of step 1 that
turned out to be missing (§4). The flagship: the one screen Komga structurally
cannot have. Reviewed alone, because it carries
every risky claim in the UI half and the one server addition.

**PR C — users and backup.** C4 §9 steps 6 and 7. CRUD over routes that exist,
plus C4 §5's load-bearing copy.

The alternative considered and rejected was **two PRs with the flagship
first**, which is what the server half did in reverse. It puts the new
settings chrome inside the largest PR of the three and makes the flagship the
first thing reviewed rather than the first thing measured — the same argument
the server half's §1 made for its own ordering, one layer up.

**Out, and where each goes instead:**

- **A route for `exclude_globs` to mean anything.** O92: the column is
  settable, patchable, edited by this screen and **never read** by
  `scan::scan_root`. The roots screen edits it because C4 §2 lists it; the
  screen does not pretend it does anything, and O92 stays open. Making it work
  is a scanner change with its own blast radius.
- **A general account screen outside the reader.** `Account.tsx` mounts from
  `App.tsx` and is reachable only from inside a work. §6 records what that
  costs the users screen and does not fix it.
- **A watcher, an interval scheduler, or an in-server backup timer.** The
  server half declined all three; nothing here reopens them. The roots screen
  shows `watch::probe`'s answer and the interval; a cron entry runs
  `taiyaku library backup`.
- **An audit log.** C4 §8 declines it and nothing has produced the case.

## 2. The settings area, shared by all three

**It ships in PR A**, with the roots screen as its first and only tab. It is
described here rather than in §3 because all three PRs read `TABS` and mount
under `/config`, and a shared thing described inside one PR's section is how it
stops being read by the other two.

**One sidebar entry, and a nested route area with its own sub-nav.** C4's own
opening line settles the shape — configuration is *"touched rarely and from a
settings area"* — so the always-visible chrome grows by one, not four.

```
/config              → redirect to TABS[0]
/config/rules        RuleEditor      (PR B)
/config/roots        Roots           (PR A)
/config/users        Users           (PR C)
/config/backup       Backup          (PR C)
```

**`TABS` lives in `config.ts` and is read twice** — by the sub-nav that draws
it and by the index redirect that picks a landing screen — so a tab cannot
exist in one and not the other. Each PR appends its own entry. **The landing
tab therefore changes once**: PR A ships with Roots first because it is the
only screen there; PR B prepends Rules and becomes the landing screen, which
is the right answer once the flagship exists and is a one-line change rather
than churn.

**No client-side admin guard, deliberately.** `Routes.tsx` puts none on
`/proposals` today, and `api.ts`'s `getJson` was changed during C3 precisely so
a non-admin who types that URL is shown *the server's own sentence* rather
than a bare status. `/config/*` follows it exactly: the sidebar link is drawn
for admins only, every route is reachable by URL, and every screen renders the
403's message. The alternative — a `<RequireAdmin>` wrapper — would put a
second opinion about who may administer into the client, three months after
O93 closed by making `session::administering_or_forbidden` the only function
in `taiyaku-serve` that holds one. A client that draws a screen it should not
is a cosmetic bug; a client that *decides* is the shape O93 was.

**Files.** `Config.tsx` is the area shell — the sub-nav and an `<Outlet/>` —
and `config.ts` beside it holds `TABS` and the roots/scan client, following
the convention `gate.ts` states for itself: *"`library.ts` beside
`Library.tsx`"*. `Config` is drawn inside `Shell`, so the sidebar, the status
context and the browse preferences are unchanged.

## 3. PR A — roots and scan control

### 3.1 The screen

One card per root. The regime line C4 §3 specifies is assembled from what
`GET /api/library/roots` already sends — `path`, the four default columns,
`scan_interval_secs`, `keeps_documents`, `last_scan_at`, `last_scan_status` —
and the watch state, which that route does **not** carry: `watch::probe` runs
on `POST` and its answer rides on the create response alone. So a root's
`watching`/`watch_reason` are known for a root added in this session and
unknown for one loaded from the list.

**Stated rather than papered over.** The card draws the probe's answer where it
has one and says *"watch state unknown — probed when a root is added"*
otherwise. The alternative is a `GET` that re-probes every root on every list,
which turns a page load into N filesystem experiments; the honest gap is
cheaper than the dishonest number, and it is the same discipline C4 §3 states
for the scan report itself.

### 3.2 The three validations are surfaced, not re-implemented

`scan::add_root_checked` runs all three and its message names which one failed.
The screen renders that message verbatim and adds nothing:

1. **Unreadable or absent path** — the server's own sentence.
2. **Nesting, in both directions** — the server's own sentence.
3. **The watch probe** — surfaced by *showing* `watching`, `watch_reason` and
   `interval_secs` from the success body, which is the only one of the three
   that produces something on a *successful* add. C4 §2 asks for the regime to
   be known before the first scan rather than discovered later; this is what
   makes that visible.

No client-side path validation. A client that guessed at nesting or
readability would be a second implementation of a check whose whole point is
that it runs *as the server process*, which is precisely the question C4 §2
says a containerised deployment answers differently.

### 3.3 Removal states its counts and does nothing until confirmed

`DELETE /api/library/roots/{id}?dry_run=true` answers `{works, positions,
notes}` without touching anything. The confirmation draws C4 §2's sentence
from those three — *"This removes 412 works, 3 in-progress positions and 27
notes"* — and the real `DELETE` is sent only from that dialog.

**The dry run is fetched when the dialog opens, not when the card renders.** A
count per root per page load is a derivation-shaped question asked about
roots nobody is removing.

**Zero is drawn, not omitted.** A root whose removal costs nothing still says
so; *"This removes 0 works, 0 positions and 0 notes"* is a different sentence
from a dialog that mentions no counts, and the second reads as "counts not
available" to whoever is about to press the button.

### 3.4 The scan, and what the report must not hide

`POST .../scan` answers `202` and the id of the running scan; a second `POST`
against the same root answers `409` naming the one in flight, and the screen
draws that rather than a generic failure.

`GET .../scan` answers `{running, elapsed_secs, report, last_scan_at,
last_scan_status}`. `report` is `null` while running — `ScanProgress` carries
no per-file counter and this design does not add one — so **live progress is
elapsed time and nothing else**, and the screen says "scanning… 14s" rather
than drawing a progress bar over a number nobody computes. Polling at **1 s**
while `running`, stopped on the first response that is not: the route reads an
in-memory registry entry plus one indexed row, so a second is generous rather
than tight, and the walk it is watching is measured in tens of seconds at
best. The interval is a guess; nothing has measured what a full scan of a real
root costs, which is the gap both spec B and the server half's build design
already carry.

**`ScanReport.tsx` renders both structures whole, in the two sections the
server already separates them into** — `scan` (the walk) and `derive` (what
was made of it). C4 §3's discipline is the whole of this component's contract:

- **Every field is drawn, including a collection that is empty this time.**
  The route destructures both reports with no `..` for exactly this reason,
  and the client must not undo it at the last step. A `Vec` that is empty
  renders as `0`, never as a section that vanishes: *"nothing unreadable this
  time"* and *"this field does not exist in this version"* have to look
  different, and a missing row is how they stop looking different.
- **The did-not-do fields lead.** `refused`, `unreadable_dirs`, `unreadable`,
  `suppressed`, `unrecognised_kinds`, `unfoldable`, `unreadable_hand`,
  `unwritable_documents`, `stale_documents`, `lost_positions` and
  `orphan_documents`/`declined_documents` come before the counts of what
  succeeded. A scan that quietly ignored 40 EPUBs and said "done" is the
  failure this project keeps writing rules against, and burying the confession
  under the achievement is the readable version of the same thing.
- **`root_removed_mid_derive` and B §8.1's refusal are drawn as refusals**,
  not as zeroes among counts.
- **The `note` the route sends is drawn.** It says this process's own
  `/api/works` list is a startup snapshot; the screen is where somebody would
  otherwise meet that discrepancy with nothing explaining it.

The two document lists — `orphan_documents` and `declined_documents` — are
tables, since each entry carries a path, a title and (for an orphan) the work
that absorbed it. Everything else is a definition list of counts.

### 3.5 Files

```
web/src/config.ts        TABS, Root/Added/Removal/ScanStatus types, the client
web/src/Config.tsx       the area shell: sub-nav + <Outlet/>
web/src/Roots.tsx        the list, the add form, the reload
web/src/RootRow.tsx      one root: fields, edit, scan, remove confirmation
web/src/ScanReport.tsx   both reports, whole
```

Split on `Proposals.tsx`/`ProposalRow.tsx`'s own line and for its stated
reason: the list owns which card is expanded, the card owns what an expanded
card fetches — here the dry-run counts and the scan poll, both of which belong
to the root being acted on rather than to the page.

`Routes.tsx`, `Sidebar.tsx` and `index.css` each take one edit.

## 4. The save route — the one piece of Rust

### 4.1 `preview::apply`

`preview::preview` is `BEGIN IMMEDIATE` → snapshot → `replace_all` →
`derive_with(Documents::Skip)` → snapshot → `ROLLBACK` → diff. **A save is the
same thing without the rollback**, and with `Documents::Reconcile` in place of
`Skip`, because a real derivation reconciles hand documents and a preview must
not touch the user's tree.

```rust
pub fn apply(conn: &Connection, proposed: &[rules::Rule]) -> Result<Applied, String>
```

`Applied` carries the `Impact` (so the screen can state what the save actually
did, against what it predicted), the `reopened_proposals` ids, and the rules as
written.

**Why a function in `taiyaku-library` rather than the sequence inlined in the
route.** `replace_all` is `pub(crate)`, which is correct — nothing outside the
crate should be able to rewrite the rule table without deriving. Widening it
to `pub` for one caller would export exactly the half of the operation that is
dangerous alone.

**And it makes an existing test stop being a hand-typed sequence.**
`preview/tests.rs`'s `a_preview_and_a_real_save_produce_the_same_structure` —
the test C4 §7 leads with, and the one the whole preview mechanism is chosen to
make cheap — currently writes its "real save" side as `replace_all(&conn,
&proposed)` followed by `derive(&conn)`, two lines that exist nowhere in
shipped code. After this it calls `apply`, so the claim becomes *the preview
agrees with the save this product actually performs* rather than *with a
sequence this test types out*. That is the same failure mode
`plans-need-executed-fixtures` names, sitting inside the test that guards the
screen's central promise.

### 4.2 `PUT /api/library/rules`

```
PUT /api/library/rules    { "rules": [ …the full proposed list, ids and all… ] }
  → 200 { "impact": {…}, "rules": […], "reopened_proposals": [ids] }
```

Symmetric with `POST .../rules/preview` down to the body key, the
`MAX_PROPOSED_RULES` bound and the `rule_from_json` reader — the same list
previews and saves, so the client cannot send one shape to one and another to
the other. On `Library::working()`, not `Library::conn()`, for the reason every
long job in this module takes a second connection.

**`PUT`, not `POST`**, because the body is the complete desired state of a
collection that already has a URL, which is what `PUT` means; `POST
.../rules/import` stays what it is — a *document* import that mints fresh ids
by design, for a document from another library that has none of this one's.

**The two routes answer the same `impact` shape**, and the screen compares
them: a save whose impact differs from the preview it was made from is drawn as
a warning rather than swallowed. That is not expected — nothing should change
between them on a single-writer library — but the screen's entire promise is
that the preview is true, and the cheapest place to notice it is false is the
save that just disproved it.

**A new route goes in four lists, not two**, per #106's own finding:
`guarded_routes()`, the grant-posture table, the `expected` array in
`auth/tests.rs`, and the running `table.len() - driven` count whose message
carries a history line to extend.

## 5. PR B — the rule editor

### 5.1 Layout

C4 §1's: an ordered list on the left, the editor and the preview on the right.

```
┌──────────────┬──────────────────────────────────────┐
│ 1 ▸ Series   │ kind [dir_regex ▾]  match_on [rel ▾] │
│ 2 ▸ Edition  │ pattern  [Manga \(RAW\)/.*         ] │
│ 3 ▸ RAW    ◂ │ sets  series[ ] edition[raw]         │
│ 4 ▸ Colour   │       kind[ ]   language[ ]          │
│ 5 ▸ (off)    │ note [ ]   scope [every root ▾]      │
│              ├──────────────────────────────────────┤
│ [+ Add]      │ Impact │ Matched │ Provenance │ ⚠ 2  │
│ [Import]     ├──────────────────────────────────────┤
│ [Export]     │ 6 series merged · 71 works re-homed  │
│              │ 0 orphaned · 3 proposals reopened    │
└──────────────┴──────────────────────────────────────┘
              [ Save — apply to 71 works ]
```

### 5.2 Four tabs, and two of them read one array

C4 §1 names three tabs and describes conflict detection as a warning; the
server half's §2.1 calls it a tab. It is a **fourth tab with a count badge
drawn from every tab**, so a conflict is never hidden behind a tab nobody
clicked — which is the failure mode a warning-that-is-a-tab has and a warning
does not.

- **Impact** — `impact`, independent of which rule is selected.
- **Matched files** — `rows`, as `rel_path`, `before → after`, and
  match-or-near-miss. The near-misses are the point: C4 §1's own example is a
  colour-edition regex failing on `Yu Yu Hakusho - Colored (Colored Council)`,
  and `preview::rows_for` already implements a definition of near-miss and
  documents it.
- **Provenance** — the *same* `rows`, read the other way: `rel_path` × the
  seven fields, each cell naming the deciding rule or the root's own default.
  `Decided` is an index into the list the request carried, never a rule id —
  the boundary `config_api`'s own module doc explains — so the cell names the
  rule at that position in the editor, which is the rule the person is looking
  at.
- **Conflicts** — `conflicts`, each naming the field, the path, the losing and
  winning rule positions, the count of *other* disagreeing rules, and
  examples. C4 §1's two offered fixes — reorder, or narrow — are the two
  actions on the row.

`rows` is scoped to `selected`, and `MATCHED_FILES_LIMIT` caps it at 500 with
`rows_truncated` saying so. **The cap is drawn.** A table that quietly stopped
at 500 reads as the whole list, which is the discipline `Rows::truncated`
exists for.

### 5.3 What triggers a preview

Measured on the real library — 130 series, 1858 works, release build — a
preview of the unchanged rule list costs a **median of 281 ms**, and one with a
rule appended **495 ms**, with `grouping` 1.3 ms of it. The derivation is the
entire cost. So, decided with the owner 2026-09-04, and the server half's §2
already pre-approved it: **the editor previews on commit, never while typing.**

A preview runs on: **field blur or Enter · an enable toggle · a drag-reorder ·
an add · a delete · selecting a different rule in the list.**

The last is the one that is not obvious. `rows` is scoped to `selected`, so
changing which rule is open is the only way to populate the Matched files and
Provenance tabs for it — and it is one gesture, not a stream, which is exactly
the test the server half's §2 applies to a drag-reorder. A person clicking down
a five-rule list pays five derivations; a person typing a regex pays none.

**The alternative was an explicit Preview button**, which is the most literal
reading of "on demand" and was rejected on what it costs the screen: the tabs
and the Save count go stale against the editor, so the screen has to render and
explain a fourth state, on the one screen whose whole promise is that the
preview is true. **A long debounce on typing** was rejected because a
half-typed regex is a rule that matches nothing, and deriving the whole library
to answer a question about a pattern nobody has finished writing holds the
write lock for 0.3–0.5 s against a concurrent scan, repeatedly, for nothing.

**Nothing is written until Save**, per C4 §1, and the button states what it
will do from the live preview's `works_rehomed`. Because the preview is never
stale, that number is never a lie.

### 5.4 Conflicts, and the count that keeps a chain honest

Reported where two enabled rules would decide a field **differently** — the
server half's §2.1 narrowed C4 §1's literal wording, because taken literally it
flags the shipped seeded defaults, where rule 1 and rule 4 both set `Series` by
design. A chain of three or more disagreeing rules reports its most recent
disagreement and the count of the others; the row reads *"and 2 others"* rather
than implying it is the whole story, and in the ordinary two-rule case the
count is zero and the editor draws nothing extra.

**The conflicts term is quadratic in the number of proposed rules and the
preview route bounds the input at 200**, measured in a debug build on a
one-work fixture: 10 extra rules 47 ms, 40 → 401 ms, 80 → 1.34 s, 160 →
5.01 s. The editor draws a rule count and, past **forty rules**, says the
preview is getting expensive. Forty is a guess anchored to the measured curve's
knee, not a measurement of this screen.

### 5.5 A rule an accepted proposal wrote

C4 §1 requires those rules marked, and deleting one to reopen the proposal it
came from. Both halves are reachable without a server change:

- **The mark** comes from `GET /api/library/proposals`, whose `Proposal`
  already carries `created_rule_id` — `curation.ts` types it, and its own
  comment already anticipates *"an `accepted` proposal whose rule was since
  deleted through C4's editor"*. The editor joins on it and draws the badge.
- **The reopening** is already what the store does. `DELETE .../rules/{id}` and
  the new `PUT` both answer `reopened_proposals`, and the screen states the ids
  after the save rather than predicting them before it — the prediction is
  available from the preview's `proposals_reopened` count, which is drawn on
  the Impact tab exactly as C4 §1 asks.

**"Proposals resolved" is not drawn, because it is not reportable.** C4 §1
lists it; a derivation cannot produce it, and the server half's §2 records why
in full. The Impact tab draws `proposals_reopened` and `proposals_raised`.

### 5.6 Import and export

`POST .../rules/import` takes the export document, which is a bare array of
`{ordinal, enabled, root_path, kind, pattern, match_on, set_*, note}` — note
**`root_path`, not `root_id`**, because a document carried between libraries
cannot mean anything by a bare number. So the client converts in both
directions against the roots list `config.ts` already fetches, in **one
converter used by both**, so an export and an import cannot disagree about the
mapping.

**Import is drawn as the destructive operation it is.** It replaces the whole
set with fresh ids, which reopens every accepted proposal whose rule it
replaces — the route reports the ids and the screen states them. That is
correct for an import of a foreign document and would be wrong for a save,
which is why §4 exists and why the two are different buttons with different
wording.

### 5.7 Files

```
crates/taiyaku-library/src/preview.rs      + apply, + Applied
crates/taiyaku-library/src/preview/tests.rs  the existing test calls apply
crates/taiyaku-serve/src/config_api.rs     + save_rules
crates/taiyaku-serve/src/routes.rs         the route, in four lists
crates/taiyaku-serve/src/config_api/tests.rs

web/src/rules.ts        Rule/Impact/Row/Conflict types, the client, the
                        root_path↔root_id converter
web/src/RuleEditor.tsx  the screen: the list, the selection, the preview state
web/src/RuleList.tsx    the ordered list, drag-reorder, enable
web/src/RuleForm.tsx    one rule's fields
web/src/PreviewTabs.tsx the four tabs
```

## 6. PR C — users and backup

### 6.1 The refusals are two invariants and one actor rule

C4 §4 calls them "three refusals" and a screen that reads them as three ships
fewer than three:

1. **The last administrator cannot be deleted** — and the same invariant
   covers **demotion**, through a second route. Same lockout, and
   `account::update` enforces it there too. A screen that disables only the
   Remove button on the last admin leaves the role dropdown open on the
   identical accident.
2. **An administrator cannot demote themselves**, which fires *even beside a
   second administrator* and so is a rule about the actor rather than about
   the population. `acting_as` is what carries it.
3. **A password reset keeps the session that made it** — A2 §4's rule, and this
   screen must not have a second opinion. On this screen a reset is an
   administrator resetting *somebody else's*, so the store ends the target's
   sessions and leaves the caller's alone.

And a fourth, found by #106's own whole-branch review: **an administrator
cannot reset their own password here at all**, because the justification for
ending the target's sessions is exactly false when the target is the caller.
The route refuses it and names `/api/password`.

**The server is the authority; the client disables and explains.** Every one
of these is enforced in `taiyaku-library` and answered as a `409` with a
sentence. The screen renders that sentence when it arrives, and *additionally*
disables the control with the reason beside it where it can tell in advance —
which is both invariants on the last admin, and the self-demotion. The
duplication is deliberate and one-directional: the client may refuse something
the server would also refuse, and may never permit something the server would
not.

**The self-password-reset refusal names a screen that is not routable.**
`Account.tsx` mounts from inside the reader, on `/read/:work`, so "change your
own password at the account panel" is advice a person cannot follow from
`/config/users`. The screen says where it is; making it reachable is a change
to the reader's chrome and is out of scope here. Worth an open item on the way
out.

### 6.2 The users screen

One card per account: name, role, `can_download`, `can_stream`, granted roots,
and **`can_log_in`** — an account still holding `credential::LOCKED` has a row
and no way in, and `account::list` calls that "the second half worth printing".
Expanding shows its sessions (created, last seen, user-agent, IP, and whether
it is the browser doing the asking) and its API keys, each revocable
individually, because C4 §4's stated case is "a device was lost".

Removal takes `?dry_run=true` first and states all five counts plus the name,
in the same shape and for the same reason as §3.3.

The root-grant editor needs the roots list, which `config.ts` already fetches
for §3 — one client, two screens, rather than a second fetch that could
disagree.

### 6.3 The backup screen

`[Back up now]`, the list newest-first with a download link each, and C4 §5's
copy, which is load-bearing rather than decorative:

- **Restore is out of band.** Stop the server, move the file, start it. The
  screen says so instead of offering a button, per C4 §5.
- **A backup is the whole database, password hashes included.** `backup_api`'s
  own module doc says this is worth naming because *"the admin screen has a
  download button" does not, on its face, look like "the admin screen hands out
  the password file"*. The screen names it.
- **A sidecar export is not a backup.** Five documents per work and none of the
  cross-work data — collections, tags, external ids, users, read history. C4
  §5: that distinction is invisible unless something says it.
- **There is no schedule here.** A cron entry runs `taiyaku library backup`,
  and `--keep` is where retention lives. The screen states the command rather
  than offering a control the server half deliberately declined.

Two runs inside one second collide and the second is refused `409` — the name's
timestamp has one-second resolution. The screen draws that as the timing it is,
not as a fault.

The download is a plain `<a href download>`: the route sends
`Content-Disposition: attachment` and the session cookie rides on the request,
so fetching it into memory first would buy nothing and cost the file's size in
JS heap.

### 6.4 Files

```
web/src/users.ts     User/Session/Key/Removal types, the client
web/src/Users.tsx    the list, the add form
web/src/UserRow.tsx  one account: fields, sessions, keys, removal
web/src/backup.ts    Backup type, list/take/download
web/src/Backup.tsx   the screen and C4 §5's copy
```

## 7. Testing

Every **screen** test lands in the `jsdom` project, per C4 §7 and C1 §9 —
nothing here touches the resolver, and nothing here has a layout question the
`browser` project would be needed for. The pure modules' tests are `.ts` and so
land in `node`, which is where `vite.config.ts` sends them by extension rather
than by choice: `node` includes `src/**/*.test.ts` and `jsdom` includes
`src/**/*.test.tsx`.

Every C4 §7 test that falls in the UI half:

- **Adding a nested root is refused, in both directions**, and the screen draws
  the server's sentence naming which check failed.
- **Removing a root reports work, position and note counts before acting**, and
  sends no `DELETE` until the dialog is confirmed.
- **The last admin cannot be deleted, and an admin cannot demote themselves** —
  asserted on the *controls*, since the store's own tests already cover the
  refusals: the Remove button and the role control are both disabled on the
  last admin, and the role control is disabled on the caller's own card.
- **A scan report lists unreadable formats rather than omitting them**, with an
  empty list rendering as `0`.
- **Reordering two rules changes the preview**, which is per-field layering
  asserted through the screen rather than assumed by it.
- **Deleting a proposal-written rule states the proposal it reopened.**
- **Conflict detection's seeded-defaults case is drawn** — `edition = raw`
  under `Manga (RAW)/` against a later `edition = en-color`, both matching
  `Manga (RAW)/Go-Toubun no Hanayome - Full Color`.

And six this design adds:

- **`preview` and `apply` produce the same structure** — the existing
  `preview/tests.rs` test, now driving `apply` rather than a typed-out
  sequence, which is what makes it a claim about shipped code.
- **`PUT /api/library/rules` derives**, asserted by reading the grouping back
  after a save rather than by trusting the impact it reported.
- **A save of the unchanged list reopens nothing**, which is the property
  `replace_all`'s differential update exists for and the one an import breaks.
- **Typing in a rule field fires no preview request**, and a blur fires exactly
  one. The screen's central cost decision, asserted rather than described.
- **A root that has never been scanned draws "never scanned"**, not `null` and
  not a blank.
- **A second scan of a root in flight draws the 409's naming of the one
  running.**

The `browser` project gains nothing. What `node` gains is `config.ts`,
`rules.ts`, `users.ts`, `backup.ts` and the `root_path` converter — the
conversions and the `TABS` reading, which is where they belong and where they
are cheapest to pin.

## 8. What is deliberately not done

- **No progress bar.** `ScanProgress` carries elapsed time and a done flag;
  drawing a bar over a number nobody computes would be an invented figure on
  the screen whose own discipline is that a report names what it did not do.
- **No re-probe of the watcher on list.** §3.1.
- **No client-side path, nesting or pattern validation.** The server owns all
  three, and a regex that compiles in JS is not the regex the store compiles.
- **No rule testing against typed-in paths.** C4 §8 declines it; the preview
  runs against the real `scan_entry` table.
- **No `exclude_globs` behaviour.** O92; §1.
- **No self-service anything.** C4 §8: this is the administrative view of
  other people.

## 9. What this invalidates

- `docs/status.md` — the test count at **both** of its sites, the `web/` prose,
  and `taiyaku-serve`'s route prose for the new `PUT`.
- `README.md` — configuring a library stops being a CLI-only activity.
- `scripts/coverage-floor.mjs` — the per-file floors and the file count, on all
  three PRs.
- `web/vite.config.ts` — the aggregate thresholds, if they move.
- `docs/notes.md` — anything measured on the way, per status.md's standing
  promise that every figure traces to one.

## 10. What could not be measured

- **What the editor costs on a library of thousands.** 281 ms and 495 ms are
  measured on 130 series and 1858 works. C4 §11 named the library of thousands
  and it is still unmeasured; §5.3's per-commit trigger is chosen to make the
  answer matter less rather than to depend on it.
- **Whether forty rules is the right place to warn.** §5.4 — a guess anchored
  to a debug-build curve measured on a one-work fixture, not a measurement of
  this screen.
- **What a full scan of a real root costs**, which is what §3.4's 1 s poll and
  C4 §3's interval defaults should both be chosen from. Spec B and the server
  half both carry this same gap, and the roots screen is the first thing that
  could close it: the first real scan run through `POST .../scan` is a
  measurement worth recording.
- **Whether the settings area needs its own empty state.** A library with no
  roots at all reaches `/config/roots` with nothing to draw, and C1 §8's empty
  states were designed for browse rather than for configuration. The add form
  is on the screen either way; whether that is enough is a thing to look at
  once it exists.

# Building the library's configuration UI — design

**Status: settled in brainstorming on 2026-09-02. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is the **build design for spec C4**, whose product design is
[`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
and is not restated here. That document settled *what* the four screens are —
the rule editor, roots, scan control, users, backup — against a live mockup
kept in `.superpowers/brainstorm/` and gitignored. This one settles **what gets
written, in what order**, and records the four places where the three weeks
since it was written have overtaken it.

## 0. What already exists, and the three things that do not

The store layer is further along than C4 §2 and §3 assume.

- **`root` already carries every column the screens edit.**
  `scan_interval_secs`, `exclude_globs`, `edition_key`, `default_kind` and
  `default_language` come from migration 1, and `keeps_documents` from
  migration 12 (#104), which post-dates the spec entirely.
- **`rules::Engine` already resolves with per-field provenance.**
  `Resolved::provenance` is a `BTreeMap<Field, i64>` naming the rule that set
  each field, with a `ROOT_FALLBACK` sentinel of `0` for "the root's own column
  decided this, and no rule did" — which is C4 §1's third preview tab, computed
  already on every resolve rather than needing to be built.
- **The watcher probe already ships.** `watch::probe`, `watch::interval_for`
  and `watch::report_line` answer C4 §3's regime line by experiment, exactly as
  spec B §8 designed. Its only caller today is `taiyaku library scan`.
- **`scan::ScanReport` and `derive::DeriveReport` already carry what §3's scan
  report must name** — entries found, unreadable containers, `suppressed`,
  `raised`, and the five document fields #100–#104 added.

Three things do not exist, and each is load-bearing for a screen.

1. **No HTTP route.** `routes.rs` registers fourteen `/api/library/*` paths and
   none of them is a rule, a root, a user or a backup.
2. **The server has never scanned.** `taiyaku_library::scan` appears anywhere in
   `taiyaku-serve` only in `test_support.rs`. Scanning is `taiyaku library
   scan`; nothing in the serving process has ever walked a tree.
3. **`taiyaku library backup` does not exist.** The store spec §13.4 designs it
   and C4 §5 says "this screen operates it". The only trace of it in the tree is
   a comment in `taiyaku-library/Cargo.toml` explaining why `rusqlite`'s
   `backup` feature is enabled.

**And the store's write surface is thinner than its read surface.** `scan.rs`
offers `roots`, `add_root(path)` and `scan_root`; there is no update, no delete
and no validation of any kind. `rules.rs` offers `read`, `export_json` and
`import_json` — the whole list at a time, which is what the CLI needed — and no
per-row create, patch or delete. Both gaps are PR 1's, and both are the ordinary
shape of a store built by the command that used it first.

## 1. Scope: two PRs, and what is out

Decided with the owner, 2026-09-02: **the server half first**, on C1's pattern
— #81 the browse API, #82 the shell, #83 the pages. The alternative considered
and rejected was a vertical slice per screen, which puts the settings-area shell
inside the largest PR of the four and makes the flagship screen the first thing
reviewed rather than the first thing measured.

**PR 1 — rules, roots, and the scan trigger.** C4 §9 steps 1, 4 and 5's server
halves: rules CRUD and `preview`, roots CRUD with the three validations and
removal counts, and `POST .../scan` with its status. It carries the risky claim
(§2), so it carries the measurement that decides whether the claim is
affordable.

**PR 2 — users and backup.** C4 §9 steps 6 and 7's server halves: user CRUD,
the three refusals, session and key revocation, and backup as a store function,
a CLI command and three routes. CRUD over tables that already exist, with no
new mechanism in it.

**Out, and where each goes instead:**

- **The four screens.** They are the UI PRs that follow these two.
- **The watcher and the interval scheduler** — spec B §8's tiers 1 and 2.
  §4 builds a trigger and nothing long-lived. A watcher per root is a
  supervised task with debounce and restart semantics, and it wants its own
  branch and its own measurement of the watch-count ceiling B §8 says must be
  reported rather than silently degraded.
- **An in-server backup schedule.** The same call, for the same reason: the
  server gains no timer at all in C4, and a cron entry runs
  `taiyaku library backup`. Store spec §13.4's *"a backup nobody remembers to
  take is not one"* is answered by the command existing and being cronnable,
  not by a timer inside a process that also serves pages.
- **O58 and O60.** Both say the fix "wants C4's permission model". PR 2 ships
  the **writer** for `can_download`, `can_stream` and `user_root` — which O54
  records as reachable from no fixture-free path today — and neither item
  closes. What a root grant *means* for progress, notes and truth, and whether
  editing shared content counts as administering, are decisions with no
  incident behind them yet.

## 2. The preview runs the real `derive`, and rolls it back

C4 §6 settles that `POST /api/library/rules/preview` takes the **whole proposed
list**, because layering makes a rule's effect a property of its neighbours.
What it does not settle is what answers it. This does.

**The mechanism**, on a connection of its own:

1. `BEGIN IMMEDIATE`.
2. Read the current grouping — every `member`'s `(work, edition)` and every
   work's series.
3. Replace the `rule` table with the proposed list.
4. Run `derive::derive_with(.., Documents::Skip)`, the same function a save
   runs.
5. Read the resulting grouping.
6. `ROLLBACK`.
7. Diff the two in memory. Series merged, works re-homed, works orphaned,
   proposals reopened and proposals raised are all differences between steps 2
   and 5 — C4 §1's Impact tab, and nothing else computes them.

> **Steps 1 and 2 were the other way round when this was written, and the
> order is load-bearing.** Reading the "before" grouping outside the
> transaction lets a writer on another connection — `Library::working` exists
> precisely so a scan can hold one — commit between the snapshot and the
> `BEGIN`, so `before` and `after` describe two states that never coexisted.
> The screen whose whole promise is that it is true would show phantom
> rehomes and orphans. Corrected 2026-09-03, from Task 3's review.

> **"Proposals resolved" is not reportable and never was.** C4 §1 lists it;
> a derivation cannot produce it. `proposals::raise` only inserts, and
> `reopen_orphaned_accepts` only flips an accepted proposal back to `open`
> — nothing in a derivation moves a proposal *out* of `open`, so the count
> could only ever be zero. It ships as **`proposals_reopened`** (the
> genuinely useful warning: this list would reopen N questions you already
> answered) and **`proposals_raised`**, decided by the owner 2026-09-02.

**Why the real `derive` and not a second evaluator.** C4 §7's first test is
that *"`rules/preview` and a real save produce the same structure"*, and the
whole screen is a promise that the preview is true. Running the same function
makes that true by construction; a preview pass of its own makes it true only
as long as a test keeps two implementations in step, which is the arrangement
this repo has repeatedly found drifts. C4 §8 already declines a second
evaluator for the neighbouring feature — *"no rule testing against arbitrary
typed-in paths … cheaper than building a second evaluator"* — and this is the
same argument one layer down.

~~It fits mechanically: **`derive` holds no transaction of its own**
(`derive.rs:408`, which says so and says why), so an outer transaction wraps it
cleanly, and `unchecked_transaction`'s plain `BEGIN` is available because
nothing beneath is holding one.~~

> **Struck 2026-09-03: this was wrong, and it was the one claim the whole
> mechanism rested on.** `derive` holds no transaction of its own, but it
> *calls* two functions that do — `import::scanned` opens one per work and
> `fold::absorb` opens one per fold — and SQLite refuses a plain `BEGIN`
> inside an open transaction. It fires on the first work any real preview
> processes, not in a corner. The cited comment at `derive.rs:408` says so
> in its own second sentence (*"`crate::import::scanned` opens one per
> work"*); this paragraph read it and drew the opposite conclusion.
>
> What makes the wrapping work is a `Nested` scope (`lib.rs`): a real
> `BEGIN` when `Connection::is_autocommit()`, a `SAVEPOINT` when something
> is already open, with both call sites repointed at it. That is a change
> to the ordinary import and fold paths — the ones that write a real
> library — and not merely preview scaffolding, which is why it was
> reviewed as such. A top-level `SAVEPOINT` takes no write lock where
> `BEGIN IMMEDIATE` does, measured, so the branch is load-bearing rather
> than defensive.

**The hazard the settled spec cannot know about, because #100–#104 post-date
it: `derive` writes to the user's library tree, and `ROLLBACK` does not undo
that.**

- `hand::rename_for_rekey` (`hand.rs:366`) renames a document under
  `<root>/.taiyaku/` when a work's `content_key` moves. It is gated on nothing
  — not on `root.keeps_documents`, not on anything else — because a rekey
  stranding a document is a defect it exists to prevent (#101).
- `derive` calls `hand::reconcile` for every root (`derive.rs:636`), which
  writes a missing document back where the root keeps them (#100's gap repair).

A rule preview that renamed and rewrote files in someone's library would be the
single worst thing in this product, and it would be invisible: the database
rolls back and the tree does not.

**So `derive` gains a documents policy** — one parameter, one body, and the
existing `derive(conn)` becomes the `Reconcile` case. The preview passes
`Skip`, which suppresses every write, rename and delete under `<root>/.taiyaku/`
and **leaves the reads on**: `hand::load` is what feeds a hand-set title into
the structure the preview claims to predict, and it is deliberately ungated so
that a rebuilt database recovers its documents. A second `derive` for previews
was the alternative and is rejected for the reason above — it is the second
evaluator wearing a different hat.

**On the single connection.** `ServeState.library` is an
`Arc<taiyaku_library::Library>` and `Library` is `{ conn: Mutex<Connection> }`
— one connection for the whole server. A preview on that handle holds the mutex
for the length of a derivation and stalls every request behind it. So the
preview opens its own connection to the same file; under WAL its write
transaction takes the write lock but leaves readers on the server's connection
reading the pre-transaction snapshot, and `busy_timeout` (`lib.rs:286`) governs
the collision with a real writer. This is the same reasoning §4 applies to the
scan, and the two are the first two callers of it.

**Measured 2026-09-03, and the answer changes the screen.** C4 §11 marked its
"34 ms" as an illustration rather than a measurement, and that was honest —
nothing existed to measure. It does now: on the real library (130 series, 1858
works, 1883 members, release build) a preview of the library's own unchanged
rule list costs a **median of 281 ms**, and one with a rule appended **495 ms**.
`grouping` is 1.3 ms of that, so the derivation is the entire cost and making
the diff cheaper would buy nothing.

**So the preview is not affordable per keystroke, and the screen previews on
demand.** C4 §6's "safe to call on every keystroke, debounced" is safe but not
affordable — one derivation per typed character, on a library *smaller* than
the 156 series its own mockup assumed. §11 predicted this and pre-approved the
remedy (*"debounce harder or preview on demand, and neither changes this
design"*), so it is settled rather than reopened. A drag-reorder stays a
trigger: it is one gesture, not a stream.

**The write lock is held for the whole derivation** — `BEGIN IMMEDIATE` across
0.3–0.5 s — so a preview blocks a concurrent scan on `Library::working`'s
connection for that long. Recorded rather than fixed: deriving outside a
transaction is the thing this design exists to avoid. Full figures in
`docs/notes.md`, "What a rule preview costs on a real library".

### 2.1 Conflict detection compares values, not authorship

**Decided by the owner 2026-09-03, narrowing C4 §1's letter to keep its
intent.** That spec says to detect *"that two enabled rules set the same field
on the same file"*. Taken literally that flags the **shipped seeded defaults**:
rule 1 and rule 4 both set `Series`, deliberately and by design, so the
Conflicts tab would open full of warnings about the product's own defaults on
nearly every file — a feature drowned on the day it arrives.

What ships reports a conflict only where two enabled rules would decide a field
**differently**. C4's own worked example is a disagreement — `edition = raw`
under `Manga (RAW)/` against `edition = en-color` for anything ending "Full
Color", both matching `Manga (RAW)/Go-Toubun no Hanayome - Full Color`, the
later winning, a Japanese colour edition labelled English — so this matches the
spec's intent where it narrows its wording.

The consequence worth stating: **two rules that agree today and diverge after an
edit are not warned about until they diverge.** That is the trade, and it is the
right one while the alternative is a tab nobody can read.

Rejected alongside it: reporting agreements as a second, quieter count. Nothing
has asked for that number, and it is one more thing for the editor to render.

**A chain of three or more disagreeing rules reports its most recent
disagreement, and says how many others there are.** Decided by the owner
2026-09-03, after a review found the behaviour was never put to them. Rules
setting one field to `"v1"`, `"v2"` then `"v3"` produce a single row naming
`"v2"` against the winning `"v3"` — the `"v1"`-versus-`"v3"` disagreement is
just as real. So the row carries a count of the *other* disagreeing rules and
reads "and 2 others" rather than implying it is the whole story; in the
ordinary two-rule case that count is zero and the editor shows nothing extra.

Rejected: leaving it silent, which under-reports with nothing on screen saying
so; and one row per disagreeing pair, which is N-choose-2 rows for a case that
is already rare and unchanged for the common two-rule one.

### 2.2 The noun is "override", and the row states rather than corrects

**Decided by the owner 2026-09-05, closing O95, and it does not change a line
of §2.1's detection.** The narrowing above was made because a literal reading
of C4 §1 flags the shipped seeded defaults; it does not reach the case the real
library actually produces, because there the two rules genuinely *disagree* —
rule 1 reads a series from the directory, rule 4 reads it from the filename,
and rule 4 winning is **what folds a colour edition into its main series**. The
screen called that a conflict and offered "Move rule 4 above rule 1", which is
the one action that breaks the fold.

`Conflict` is `Override` in the store, `conflicts` is `overrides` on the wire,
and the tab reads **Overrides** — one word end to end, rather than a screen and
the route it calls using two. `Override` also carries `first_value` and
`wins_value`, which the wire did not carry before: naming the two rules says
*that* they disagreed and never what about, and what about is the whole of the
judgement the tab now asks a person to make. The two values belong to the entry
`rel_path` names, not to all `examples` of it, for the same per-file reason
`others` takes a maximum — `preview::Override`'s own doc comment carries it.

The row offers **two options under a neutral lead** rather than one fix:
reorder, and *Edit rule N*, which puts the winning rule in the form. Narrowing
means editing a regex and no button can guess how — a "Narrow" button that
appended a lookahead would write a regex nobody asked for, and on this very
case the right narrowing is none.

**Rejected: suppressing the seeded pair.** No exemption can be written
honestly, because "these two rules are a deliberate layering" is intent and
intent is not a column, and special-casing the seeded ids goes stale the moment
somebody edits one. **Rejected: a further narrowing of the detector**, for the
same reason — what separates this pairing from a genuine mistake is not
anything the two values or the two patterns record.

`docs/superpowers/specs/2026-09-05-o95-overrides-framing-design.md` is this
decision's own build design, and `docs/status.md`'s O95 carries the
measurement: 27 of 128 directories on the real library, 21%, carry a filename
stem differing from their directory.

## 3. Removing a root is delete-plus-derive, and it reuses §2

C4 §2 calls removal *"the most destructive action in the product"* and requires
counts before it acts. The schema says something the spec's prose skips:

- `scan_entry`, `dir_state`, `rule` and `user_root` are all
  `ON DELETE CASCADE` from `root` (`003_scanner.sql`, `004_auth.sql`).
- `member.root_id` is `ON DELETE SET NULL` (`006_member_root.sql`).

So `DELETE FROM root` drops the **facts** and leaves every work, edition,
member, note and reading position standing — with `member.root_id` quietly
NULL, which O54 records as the value that excludes a member from every root
grant. A removal that stopped there would leave the library asserting works
whose evidence is gone, and would silently narrow what every granted reader can
see.

~~**Removal is therefore delete-plus-derive in one transaction**~~ — **corrected
2026-09-03, and this was the second load-bearing mechanism in this document
that did not work as written.** `derive` does not delete a stale work. Its own
comment says so: *"This is the only `DELETE FROM work` in the project"*
(`derive.rs:158`), and that one is reached solely through `fold::absorb`, which
fires when material **moves** to another work — not when its facts vanish. A
bare delete-then-derive was verified empirically to leave every work standing
with one member's `root_id` gone NULL: precisely the O54 state this whole
section exists to prevent.

**What removal actually does** is compute the doomed works from
`member.root_id` *before* the root row goes, delete them explicitly, then
derive. The counts C4 §2 promises still come from §2's machinery — apply it,
read what is left, roll back, diff — so the rule editor asking *what would this
rule change* and the roots screen asking *what would this removal leave* remain
one mechanism with two callers, and the destructive path stays exercised by the
same tests as the safe one. Only the middle step changed.

Left undone deliberately: teaching `derive` to drop any work with no surviving
member. That would generalise this and also cover a root that empties
*partially* — but spec B §8.1 refuses a wholly empty scan rather than applying
it, for exactly the blast radius that generalisation would open, so it wants
its own design round rather than arriving as a side effect of a removal.

**Spec B §8.1's empty-root refusal is the other half of this** and is already
designed: a scan finding zero entries under a root that had them is refused and
reported rather than applied. Removal is how a root that has genuinely become
empty is emptied, and it says its counts first.

## 4. The scan trigger, and the four things around it

`POST /api/library/roots/{id}/scan?full=` spawns a task on **its own
connection**, for §2's reason: a scan on the server's mutexed connection stalls
every request for the length of a walk.

**Single-flight per root.** C4 §3 requires that a running scan is not startable
twice. The registry is in `ServeState` — a set of root ids with a scan in
flight — and the second `POST` is refused with the id of the one running rather
than queued. Nothing persists across a restart: a process that died mid-scan
left no lock, and the next scan is a full one anyway.

**`GET /api/library/roots/{id}/scan` returns live progress and the last
report.** The report is `ScanReport` and `DeriveReport` rendered whole, because
C4 §3's discipline is that a scan **names what it did not do** — containers it
could not open, formats it recognised and cannot read, directories skipped by
`exclude_globs`, proposals raised, and B §8.1's refusal. Every one of those is
already a field; none of them is currently shown anywhere but a terminal.
*Not quite: "directories skipped by `exclude_globs`" was **not** a field when
this was written, and could not be, because nothing skipped anything — that
was O92. It became `ScanReport::excluded_dirs` on 2026-09-09, alongside
`excluded_files`, `bad_exclude_patterns` and `forgotten_by_exclusion`, and the
sentence is true as of then.*

**`root.last_scan_at` and `last_scan_status` are written for the first time.**
They have existed since migration 1 and **nothing in the tree writes or reads
either** — verified 2026-09-02. C4 §3's regime line quotes both. The scan path
writes them, on every outcome including a refusal, and the CLI's scan writes
them too so the two agree.

**`ServeState.works` is a startup snapshot.** After a scan the browse API is
current, because every `/api/library/*` route reads the database; the reader's
`/api/works` list is not, because it is served from that `Vec<Work>`. PR 1 does
not fix this — it states it, in the response and in this document, and the fix
belongs with whatever eventually makes the reader's list a query.

## 5. The API PR 1 ships

Exactly C4 §6's paths, minus the two families PR 2 carries:

```
GET    /api/library/rules
POST   /api/library/rules                create
PATCH  /api/library/rules/{id}
DELETE /api/library/rules/{id}
POST   /api/library/rules/preview        body: the full proposed list
POST   /api/library/rules/import         body: the JSON export

GET    /api/library/roots
POST   /api/library/roots
PATCH  /api/library/roots/{id}
DELETE /api/library/roots/{id}           states its counts; see §3
POST   /api/library/roots/{id}/scan      ?full=
GET    /api/library/roots/{id}/scan      status and the last report
```

All of it admin-gated through `curation.rs`'s existing `admin_or_forbidden`,
which is the house helper and needs no sixth variant.

**`POST /api/library/roots` runs C4 §2's three validations**, and the second is
the one with no equivalent anywhere in the tree today: the path exists and is
readable *by the server process*; it is not nested inside an existing root and
no existing root is nested inside it, in both directions; and `watch::probe`
runs immediately so the regime is known before the first scan.
*The overlapping-libraries design of 2026-11-02 has since narrowed the nesting
check to the roots of one library: a root inside or around another library's
is accepted, save one at or inside a folder another root reads as one volume.
See `2026-11-02-overlapping-libraries-design.md` §4.*

**Provenance needs one constant made public.** `rules::ROOT_FALLBACK` is a
private `const` today, and an API that returns per-field provenance has to say
"the root's own column" in a way a client can read. Either it becomes `pub` or
the response maps it to a name; the second is better, because `0` is an
implementation detail of a `BTreeMap<Field, i64>` and a wire format should not
inherit it.

## 6. Four places the settled spec has been overtaken

Recorded here rather than edited into C4, which is kept as written per this
repo's habit with settled documents.

1. **§1's preview is affordable in a way the spec could not argue.** It says
   re-derivation touches no filesystem, which spec B's architecture bought.
   That is still true of the *derivation* and is no longer true of `derive`,
   which since #100 also reconciles hand documents on disk. §2's documents
   policy restores the property the preview was resting on.
2. **§3's regime line reads two columns nothing writes.** See §4.
3. **§5's screen operates a command that does not exist.** Backup is PR 2's,
   store function and CLI first, screen later.
4. **§4's user screen is the only writer of three permission columns**, which
   is not how the spec frames it — it lists them as fields to edit. O54 records
   that `can_download` and `can_stream` "cannot be set to 0 outside a fixture
   until spec C4's user administration", so PR 2 is what makes two shipped
   guards exercisable by a person for the first time.

## 7. Testing

Every C4 §7 test that falls inside these two PRs, plus four this design adds:

- **`rules/preview` and a real save produce the same structure**, asserted by
  running both over one fixture and comparing the grouping — the test C4 §7
  leads with, and the one §2's mechanism is chosen to make cheap.
- **A preview leaves the library tree byte-identical**, over a root with
  `keeps_documents` on, a work whose `content_key` moves under the proposed
  rules, and a document present at the old key. Without the documents policy
  this test fails by renaming a file; with it, it passes — which is the
  executed fixture, not the asserted one.
- **Conflict detection finds the seeded-defaults case**: rule 4's
  `edition = raw` under `Manga (RAW)/` against a later colour rule's
  `edition = en-color`, both matching
  `Manga (RAW)/Go-Toubun no Hanayome - Full Color`.
- **Reordering two rules changes the derived structure**, which asserts
  per-field layering rather than assuming it.
- **Deleting a proposal-written rule reopens its proposal** — the other end of
  C3 §3's `proposal.created_rule_id`.
- **A nested root is refused in both directions**, and a path the server cannot
  read is refused with which of the two it was.
  *Since the overlapping-libraries design of 2026-11-02, a nested root is
  refused only within one library (§4 of
  `2026-11-02-overlapping-libraries-design.md`).*
- **Removing a root reports work, progress and note counts before acting**, and
  a removal followed by a re-read shows the works gone rather than orphaned.
- **A second scan of a root in flight is refused**, naming the one running.
- **A scan report lists unreadable formats rather than omitting them.**
- **`last_scan_at` and `last_scan_status` are written on every outcome**,
  including B §8.1's refusal — the case where the temptation is to write
  nothing.

PR 2 adds C4 §7's last line: **the last admin cannot be deleted, and an admin
cannot demote themselves**, plus a password reset keeping the session that made
it, which A2 §4 already decides and this screen must not have a second opinion
about.

## 8. What this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs` — both, on
  both PRs.
- `docs/status.md` — the test count at both of its sites, the crate/route
  prose, and O54's note that nothing can set the two capability columns.
- `README.md` — configuring a library stops being a CLI-only activity, and
  `taiyaku library backup` joins the command list.
- `docs/notes.md` — §2's preview measurement lands there, per status.md's
  standing promise that every figure traces to one.

## 9. What could not be measured

- ~~**The preview's cost.**~~ **Measured 2026-09-03** — 281 ms unchanged, 495
  ms with a rule appended, on 130 series and 1858 works. §2 carries the
  consequence; `docs/notes.md` carries the figures. What is still unmeasured is
  the library of *thousands* C4 §11 names, and the cost with conflict detection
  running alongside, which is quadratic in rules and was measured on four.
- **What a full scan of a real root costs**, which is what §3's interval
  defaults should be chosen from. Spec B carries the same gap and neither spec
  can close it without the server having scanned once.
- **Whether conflict detection is cheap at scale.** Quadratic in rules and
  linear in files; nobody has tried it with seventy rules, and the seeded
  defaults are seven.
- **Whether a second connection's write transaction is as invisible to the
  server's readers as WAL promises.** Asserted from the mode, not from a
  measurement of this process under load. It is the cheapest of the three to
  check and belongs beside the preview measurement.

# C3 §6 — the ComicInfo write-back: build design

**Status: settled in brainstorming on 2026-09-07.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This builds the last unbuilt piece of spec C3. The product design is
[`2026-08-14-library-curation-design.md`](2026-08-14-library-curation-design.md)
§6 and §7; the gates are in
[`2026-08-14-library-store-design.md`](2026-08-14-library-store-design.md)
§13.2. Section references below name which.

**It is the only feature in this project that writes to the user's library**,
against a stated principle in `README.md` — *"Your library is not modified.
Archives are read, never rewritten"* — and every decision here is a gate on
that one fact.

## 0. What already exists, and the five gaps

More exists than the store spec's §10 table suggests, because C2's generated
download needed most of the same parts:

| Part | Where | Built for |
| --- | --- | --- |
| `ComicInfo` from the rows, and its XML | `taiyaku-serve/src/comicinfo.rs` | C2 §3, the generated CBZ |
| A work's absolute member paths | `taiyaku-serve/src/download.rs`, `member_paths` | C2 |
| Content identity from rows | `taiyaku-library`, `project::sidecar` → `identity::content_key` | A §4.1 |
| Moving a hand document after a rekey | `taiyaku-library`, `hand::rename_for_rekey` | the scanner |
| Rewriting the sidecar and its four siblings | `taiyaku-library`, `export::work` | A §4.5 |
| A confirm that does not exist until the counts do | `web/src/UserRow.tsx` | C4 PR C |
| Reading and writing zip | `zip 7.2.0`, via `taiyaku-read` | B |

The five gaps are the five decisions in §1. Nothing here needs a new
algorithm; what it needs is the order the existing ones run in, and the
refusals around them.

## 1. Five decisions the product design left open

### 1.1 The capability an administrator can lack — the one exception to O86

C3 §7 asks for this directly: *"`stamp-comicinfo` wants its own capability
rather than riding on the admin role — it is the only route in the product
that modifies files the user owns, and a capability is what lets an
administrator exist who cannot do it."*

**That is not compatible with O86 as written.** `Principal::from_account`
decides that `role` dominates every other column — `can_download: admin ||
account.can_download`, and the same for `can_stream` and `roots` — so an
administrator is unrestricted in every dimension the schema has. The two
cannot both hold, and the resolution is not to pick the more recent one.

**What ships: `can_stamp`, read unmodified by role, and O86 keeps its
scope.** Migration 013 adds
`ALTER TABLE user ADD COLUMN can_stamp INTEGER NOT NULL DEFAULT 0`.
`Principal.can_stamp` is the column, not `admin || column`.

The reason is in O86's own argument rather than against it. O86 exists because
an administrator edits rows that no grant can scope — `series_title` is keyed
`(series_id, title)` with no per-work column — so a half-sighted admin could
rename a series for the whole library while refreshing only the works the
grant shows. **Every capability O86 dominates widens what an account may
read.** This one does not: it decides whether an account may rewrite files
taiyaku does not own, and no incoherent half-state follows from withholding
it, because the operation is per work and either happens or does not.

`Principal::from_account`'s doc comment states this exception and why, and
`auth::tests` pins it — an admin with `can_stamp = 0` is refused. A comment
asserting a guard that does not exist is O95's third-pass shape and #111's
second finding; the test is what stops this becoming a third instance.

**Default 0, so nobody gains it on upgrade** — including the administrator
who applies the migration. Granting is a deliberate act on the Users screen,
and an administrator may grant it to themselves, which is two steps rather
than none.

### 1.2 A stamp is every zip member of every edition of one work

C3 §6 says *"per work, never bulk"*, and its example — *"9 archives, 4.1
GB"* — is more than one edition's worth. So the unit is the work and the
reach is total within it.

`comicinfo::read` already takes an `edition_key`, so `LanguageISO`, the
credits and the `Notes` label vary per edition for free. Series, number and
title are shared because it is the same volume.

**`PageCount` is `member.pages`, not the edition's total.** A ComicInfo.xml
describes the archive it sits in. An edition split across three archives gets
three different counts, and the download's own call — which passes the
generated CBZ's page count for a single generated file — is not the same
question.

**`rar` and `7z` members never enter a plan.** C3 §6 says the feature is
*"absent, not merely disabled"* for them. Read per member: a work holding
four zips and one rar stamps the four and says so; a work holding no zip
member at all has no control, not a disabled one.

The cost accepted: a work holding both a fan edition and a 430 MB-per-page
archival edition rewrites both, and §1.3 is what keeps that from being paid
twice.

### 1.3 The plan is computed by reading each archive, not from a column

The first shape of this decision was a `member.stamped_sha256` column
recording what taiyaku wrote, so that a later scan could recognise its own
output and decline to promote `member.chapter_source` from `filename` to
`comicinfo`.

**It was designed against a loop that does not exist, and the check is worth
recording because the column had already been specified.** Nothing in this
codebase reads a chapter number out of a `ComicInfo.xml`.
`ChapterSource::Comicinfo` is a value the *format* carries —
`librarydoc.rs`'s `from_json` parses it, so a person can hand-write it into a
`.library.json` — but the scanner's only ComicInfo read is
`taiyaku-cli/src/credits.rs`, which pulls `<ScanInformation>` for the
scanlation group, and `comicinfo::xml` writes a group as `<Publisher>`. The
round trip is open at neither end.

So no column, and no scanner branch, per the standing rule that a decision
waits for evidence while a defect does not.

**What replaces it is better than what it replaced.** `stamp::plan` opens
each zip's central directory, reads its `ComicInfo.xml` entry if it has one,
and compares those bytes to what `comicinfo::xml` renders now. Only archives
whose content would actually differ enter the plan. That is true after a
database rebuild and true when somebody hand-edited the file, neither of
which a column survives; it needs no migration; and it makes a re-stamp after
a one-field edit name two archives rather than nine. A plan with nothing to
do is an empty plan, reported without copying 4.1 GB to discover it.

The cost: a plan opens every zip member of the work. That is a central
directory read and one small entry per archive, not a page decode — the same
order of work as a scan's own pass, and paid once per confirmation rather
than per stamp.

### 1.4 Stage one, rename one; the database moves once, at the end

§13.2 requires that the new `sha256` propagate *"into the sidecar, the member
row and `content_key` in one transaction, or the write does not happen"*.

**The database parts do. The file parts cannot, and this document says so
rather than implying an atomicity that is not available.** A rename is not a
SQLite statement and cannot join a SQLite transaction. Three shapes were
considered:

- **Stage all, rename all, roll back by restoring originals.** The literal
  reading. Rejected on disk: peak usage is twice the whole work — 8.2 GB
  transient for §6's own example — on a NAS chosen for being small.
- **A transaction per member.** The database never disagrees with the disk by
  more than one file. Rejected because a half-stamped work then reads as
  fully consistent, so an interrupted run leaves nothing to find.
- **Stage one, rename one; one transaction at the end.** What ships.

Peak extra disk is one archive. On a crash, some files are stamped and their
member rows are stale — **which is exactly what `member.sha256` exists to
detect.** The scanner already reports the work stale and a rescan reconciles
it, so the recovery path is machinery that shipped rather than machinery this
feature invents. The window is named in §7 as a known cost, not hidden.

Order, once every rename in the plan has succeeded:

1. One transaction: each `member.sha256`, then `work.content_key` from
   `identity::content_key(&project::sidecar(conn, work_id)?)` — which reads
   the rows just written, so the hash algorithm is not restated here.
2. After it commits, because both derive from committed rows:
   `hand::rename_for_rekey(conn, work_id, &old_key)` moves
   `<content_key>.library.json`, and `export::work(conn, work_id, user_id,
   None)` rewrites the sidecar and its four siblings.

`user_id` is the acting principal's, because `export::work` projects that
user's notes into the sidecar it writes.

**Cached covers and thumbnails are content-addressed by the old `sha256`**
(`serve/cover.rs`, `cache_path` and `thumb_cache_path`), so they become
unreferenced and regenerate on demand. §13.2 already names this as the cost;
nothing deletes them, because a cache entry no key addresses is not a
correctness problem and the cache root has its own eviction question.

### 1.5 The operation lives in `taiyaku-library`, and `comicinfo.rs` moves down

The four steps in §1.4 that must not diverge — member rows, `content_key`,
the hand document, the sidecar — are all `taiyaku-library`'s, and `derive.rs`
performs the same sequence for a rescan. Assembling them one crate away, in
an HTTP handler, is where the two versions drift.

So: **`taiyaku-serve/src/comicinfo.rs` moves to
`taiyaku-library/src/comicinfo.rs`**, and a new
`taiyaku-library/src/stamp.rs` owns plan → rewrite → transaction → rekey →
re-export. `taiyaku-serve` keeps two thin routes.

`comicinfo.rs` reads the database and renders XML. It is a store concern that
sits in `taiyaku-serve` only because C2's download route happened to be its
first caller. Moving it takes its tests with it and leaves the download
calling `taiyaku_library::comicinfo`; the module's visibility widens from
`pub(crate)` to `pub`.

**`zip` gets named in `taiyaku-library/Cargo.toml`.** It is already in the
tree via `taiyaku-read`, whose own manifest sets the precedent for naming a
transitive dependency directly and saying why. `docs/status.md` states this
workspace's short dependency list as a fact about the project, so that file
changes too — see §9.

## 2. The refusals, and where each one lives

Four, in this order, and the order is the whole of what keeps a refusal from
answering a question the caller was not entitled to ask:

1. **Authentication** — every `/api` route already resolves a `Principal`
   before it runs (A2).
2. **Admin**, as every other curation route.
3. **`refuse_if_cannot_stamp`**, in `routes.rs` beside
   `refuse_if_cannot_download` and `refuse_if_cannot_stream`, reading
   `Principal.can_stamp` — the field O86 does not touch.
4. **Visibility** — the work must be within the caller's roots, which for an
   administrator is unrestricted per O86, and O86 still decides that.

**The capability comes before the slug is resolved, and an earlier draft of
this section had it the other way round.** That draft ordered visibility
third *"so a work the caller may not see 404s rather than admitting it exists
with a 403"*, which is the right instinct applied to the wrong pair: the 403
here is a fact about the *account*, identical for every slug, so answering it
first tells the caller nothing. Resolving the slug first is what leaks —
an account without `can_stamp` would then get a 404 for a work it cannot see
and a 403 for one it can, which is an existence oracle for anybody holding
the admin role and not the capability, exactly the account §1.1 exists to
make possible.

A fifth guard is not a refusal about the caller and belongs to the `POST`
alone: **the body carries the plan the screen was shown, and the server
refuses one that no longer matches what it would compute now.** This is C3
§3's principle applied to the second route that acts on a client's
description of an action — *"`accept` taking the rule in the body rather than
deriving it server-side is what makes §3's preview honest: the client shows
the rule, the server writes what it was shown."* Here the stake is higher: a
plan recomputed after the fact could rewrite an archive whose size the person
never saw.

## 3. The store: a new `stamp.rs`

Two public functions and one type.

```rust
pub struct Planned {
    pub member_id: i64,
    pub edition_key: String,
    pub path: PathBuf,     // absolute
    pub bytes: u64,        // the archive as it is now
}

pub struct Applied {
    pub members: Vec<(i64, String)>,  // member id, its new sha256
    pub content_key: Option<String>,  // Some when the hub edition moved
    pub document_moved: bool,         // what rename_for_rekey answered
}

pub fn plan(conn: &Connection, work_id: i64) -> Result<Vec<Planned>, String>;
pub fn apply(conn: &Connection, work_id: i64, user_id: i64,
             plan: &[Planned]) -> Result<Applied, String>;
```

`plan` resolves each member's absolute path the way `download::member_paths`
does — the root joined to `member.path`, with the scanned-work case where
those paths are already absolute — skips every member whose `format` is not
`zip`, renders `comicinfo::xml` for that member's edition and page count,
reads the archive's existing `ComicInfo.xml`, and keeps the member only if
the two differ.

`apply` re-derives the plan and refuses if it does not match the one passed
in, then performs §1.4's sequence. It returns what changed, which is what the
screen reports and what the tests assert on.

**"Does not match" is the whole `Planned` list, ordered, compared field for
field** — member id, edition key, path and byte count — and not a count or a
set of ids. A weaker comparison passes for exactly the case the guard exists
to catch: an archive that grew between the plan and the confirmation is still
the same member at the same path, and its size is the number the person read
before agreeing.

## 4. The rewrite itself

Per archive, into a sibling temp in the target's own directory — the same
filesystem, so the rename is atomic:

- `zip 7.2.0`'s `ZipWriter::raw_copy_file` copies each existing entry with
  its **compressed bytes intact**. Pages are byte-identical and the archive
  does not inflate.
- Any existing `ComicInfo.xml` entry is dropped rather than copied; the new
  one is written in its place. Every other entry keeps its order.
- Then rename over the original.

**This is deliberately not `cbz.rs`'s approach, and the difference is not an
inconsistency.** `cbz.rs` generates a new archive and chose `STORE` so that
`Content-Length` is exact before the first byte, which is what makes `Range`
work for an OPDS client. Neither reason reaches a rewrite: nothing streams
here, and `STORE` would inflate 97.9% of this corpus's entries — the figure
`cbz.rs` measured on 2026-08-23 — turning a metadata edit into a size
increase across the library.

## 5. The server: two routes

```
GET  /api/library/works/{slug}/stamp-comicinfo
POST /api/library/works/{slug}/stamp-comicinfo
```

`GET` returns the plan: per archive its path, size, format and edition, and
the total. An empty list is a 200, not a 404 — "nothing to do" is an answer.

`POST` takes the plan in its body and performs it, per §2's fifth guard.

Both live in a new `taiyaku-serve/src/stamp.rs` rather than in `metadata.rs`,
because C3 §6's whole argument is that this is not a save path: *"putting it
on the same Save button would make the irreversible thing a side effect of
the reversible one."* Sharing a module with the `PATCH` routes is the same
mistake one layer down.

## 6. The screen

A section in `MetadataEditor`, below Save and never part of it.

It follows `UserRow`'s pattern, whose doc comment states the rule this needs:
**the confirm does not exist until the counts do.** One control; pressing it
fetches the plan; only then does the confirm render. What it names, per C3
§6:

- every archive it will rewrite, and the total size;
- the three consequences in the terms that bite — the sidecar's staleness
  check is invalidated, every thumbnail of those members regenerates, and the
  work's `content_key` changes;
- and the `rar`/`7z` members it is leaving alone, when there are any, so that
  "4 archives" is not read as "all of them".

The section is absent — not disabled — when the account lacks `can_stamp`,
when the work has no zip member, or when the plan is empty.

`UserRow` gains a third checkbox and `users_api` accepts `can_stamp` in the
`PATCH` body, beside `can_download` and `can_stream`.

## 7. What this does not do

- **No bulk stamp**, per C3 §6. Not for a series, not for a root, not on a
  schedule.
- **No un-stamp.** The original bytes are gone after the rename, and keeping
  a copy is the disk cost §1.4 rejected. A person who wants the old file back
  restores it from their own backup.
- **No write to `rar` or `7z`**, per C3 §6.
- **It does not close the crash window.** An interrupted run leaves stamped
  files and stale member rows until a rescan. That is §1.4's accepted cost
  and it is stated on screen, not only here.
- **It does not evict the orphaned thumbnails** it creates. They are
  unreferenced, not wrong.
- **It does not add a ComicInfo metadata reader**, which is what would make
  §1.3's rejected column necessary. If one lands, §1.3 is the entry to read
  first.

## 8. Testing

Beyond the usual per-route refusal and round-trip coverage:

- **A stamped archive's page entries are byte-identical to the original's.**
  Asserted over the bytes, not inferred from `raw_copy_file`'s name.
- An archive that already holds a `ComicInfo.xml` has it **replaced, not
  duplicated** — the entry count does not grow.
- An archive that holds none gains exactly one.
- `rar` and `7z` members never enter a plan, and a work with no zip member
  plans empty.
- A member whose rendered XML already matches is **absent from the plan**,
  which is §1.3's whole claim.
- An interrupted run — renames applied, transaction not reached — leaves the
  work reported stale by the scanner.
- `content_key` moves, and `<content_key>.library.json` moves with it.
- **An admin with `can_stamp = 0` is refused.** This is §1.1's exception to
  O86 pinned by a test rather than asserted by a comment.
- **The plan-mismatch refusal fails when the guard is removed.** Written
  against the guard itself, not against a screen state that happens to
  co-occur — the two double-press tests on #112 proved `disabled` rather than
  the ref they named, and this is the same shape.

Hand-applied defects per the standing rule: each test above gets its defect
applied by hand and watched to fail before the branch is offered.

## 9. Documents this changes

Listing them here because O95's own §6 listed the docs to update and missed
the design for the very screen that changed.

- `docs/status.md` — the dependency list gains `zip` for `taiyaku-library`;
  the C3 row of "What is built"; test counts.
- [`2026-08-14-library-store-design.md`](2026-08-14-library-store-design.md)
  — §10's table row for C3 is stale independently of this work (it reads "§5
  metadata editing and §6 the ComicInfo write-back not built" when §5 shipped
  as #91) and C4's row carries no shipped marker though #112 finished it.
  Both are corrected on this branch.
- [`2026-08-14-library-curation-design.md`](2026-08-14-library-curation-design.md)
  — §6 and §7 gain their shipped markers.
- [`2026-08-14-library-auth-design.md`](2026-08-14-library-auth-design.md) —
  the capability list gains `can_stamp` and the O86 exception.
- [`2026-09-04-c4-screens-design.md`](2026-09-04-c4-screens-design.md) — the
  Users screen gains a third checkbox.
- `docs/notes.md` — the measurements §11 asks for.
- `README.md` — *"Archives are read, never rewritten"* becomes true only by
  default, and the sentence has to say so.

## 10. What this invalidates

- **`README.md`'s principle**, narrowed rather than deleted: archives are
  read and never rewritten **unless an account holding `can_stamp` confirms a
  stamp on one work**. A principle with a named exception is worth more than
  one quietly false.
- **Any recorded `sha256` for a stamped member**, by construction — that is
  the feature, and §1.4 is how the record catches up.
- **`docs/mutants-baseline.md`**, for `taiyaku-library`, which gains a
  module. The run log wants a row after the branch's sweep.

## 11. What could not be measured

- **The cost of a plan on the real library.** It is one central-directory
  read and one small entry per zip member, but on the NAS over a spinning
  disk that is a seek per archive and no figure exists. Measure it against
  the real library before the screen's control is anything other than
  synchronous.
- **The cost of a rewrite at the corpus's top end.** The 430 MB-per-page
  archival volume is the case §6's size warning exists for, and nothing has
  timed a full copy-and-rename of it on the NAS.
- **Whether `raw_copy_file` preserves a data descriptor** on an entry written
  with one. The corpus's archives come from many tools and at least some
  streaming writers emit them; §8's byte-identity test is what would catch it
  failing, and it is named here because a test finding it is a different
  event from a design expecting it.

## 12. Branch shape

Two PRs, following C1's precedent — the server half first, then the screen —
because the store work is where the review rounds pay and a screen reviewed
against a moving server is reviewed twice.

- **PR A, the server.** Migration 013 and `can_stamp`; the O86 exception in
  `Principal::from_account` with its test; `comicinfo.rs` moved down and the
  download repointed at it; `zip` named in `taiyaku-library/Cargo.toml`;
  `taiyaku-library/src/stamp.rs`; `taiyaku-serve/src/stamp.rs` and the two
  routes; `users_api` accepting `can_stamp`.
- **PR B, the screen.** `MetadataEditor`'s section and its confirm;
  `UserRow`'s third checkbox; the `docs/` updates §9 lists that describe the
  screen.

**The gate, a whole-branch review and a mutation sweep run on each**, not on
the pair. This project's record is that per-task review passes while a
whole-branch pass finds Criticals, that the fix rounds themselves need
reviewing, and that the passes do not converge — so the count of rounds is
decided by what the last one found, not fixed in advance here.

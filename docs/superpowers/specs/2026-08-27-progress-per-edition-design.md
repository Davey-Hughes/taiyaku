# A reading position per edition, and O73's fold — design

**Status: settled in brainstorming on 2026-08-27. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This design closes **O73**. It exists because the measurement O73 was waiting
on was taken on 2026-08-27 and answered its question in a way that ruled out
two of the three fixes the item proposed. That measurement is recorded in
[`../../notes.md`](../../notes.md), under *"O73's cascade, measured —
2026-08-27"*, and every figure quoted below comes from it rather than from
reading the code.

## 0. What the measurement settled, in one paragraph

Accepting an `edition-of` proposal and rescanning leaves each moved volume in
the library twice: once as the new edition of the target's work, which is what
was asked for, and once still standing as its own work. Nothing in production
code deletes a `work` row. The measurement planted a reader on all four works
of the four-archive reproduction — a position, a note, a looked-at page, a box
and a labelled member digest, each written **through the API** rather than by
`INSERT` — and then deleted the two works a fix would empty. It took two
`progress` rows, two `note` rows, two `truth_page`, two `truth_box` and two
`truth_member` rows with them. So deleting an emptied work is data loss, and
the only honest fix is to move what hangs off it first.

Moving it runs into one obstacle, and this design is mostly about that
obstacle: **`progress`'s primary key is `(user_id, work_id)`**, one position
per work rather than one per edition. Folding two works into one leaves a
reader holding two positions where the schema stores one, and
`UPDATE progress SET work_id = <target>` is refused by that key — measured, not
reasoned about.

## 1. The decision: widen the key

`progress`'s primary key becomes **`(user_id, work_id, edition)`**. A reader
holds a place in each edition of a volume rather than one place per volume.

**Three alternatives were considered and lost.**

*Last write wins* — take the row with the newer `updated` and drop the other —
was the cheapest and is exactly what `put_progress` already does today when a
reader switches edition inside one work. It was rejected because the fold is
not a page turn: the two positions were recorded on two works the reader saw as
two different books in two different series, and collapsing them silently
throws away a real read.

*The target work's position wins* has the same loss and adds arbitrariness: the
colour edition a reader actually read loses to the monochrome one they opened
once.

*Refuse the fold when both works carry a position* keeps everything and pushes
the decision to a person, but it needs a refusal path on a screen whose whole
promise is that nothing changes until the next scan — and the collision happens
at the scan, not at the accept, so the screen would be refusing on behalf of a
step that has not run.

**What the widening costs, stated plainly.** It is not a schema-only change:
`taiyaku.progress.json`, `export`, `migrate` and `browse`'s `unread` count all
assume one position per work, and each is named below. It was chosen with that
cost in view.

**What it buys beyond O73.** `put_progress` currently upserts on
`(user_id, work_id)` with `edition = excluded.edition`, so a reader at slot 50
of one edition who opens a sibling edition and turns one page loses the first
position, silently, by design. After this change they do not. That is a
user-visible improvement in its own right and it is the same change; it is
noted here so it is not later mistaken for a regression in a test that pinned
the old behaviour.

## 2. Migration 009

A table rebuild, following
[`005_note_id_per_user.sql`](../../../crates/taiyaku-library/src/schema/005_note_id_per_user.sql)
line for line — that migration widened `note`'s key the same way and its
reasoning about `without_foreign_keys`, `pragma_foreign_key_check` and
re-declaring the child's own foreign keys applies here unchanged:

```sql
CREATE TABLE progress_new (
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition      TEXT NOT NULL,
  slot         INTEGER NOT NULL,
  updated      TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT,
  read_count   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, work_id, edition)
) WITHOUT ROWID;

INSERT INTO progress_new (user_id, work_id, edition, slot, updated, completed_at, read_count)
SELECT user_id, work_id, edition, slot, updated, completed_at, read_count FROM progress;

DROP TABLE progress;
ALTER TABLE progress_new RENAME TO progress;
```

**Nothing is dropped or merged by the copy**: every existing row is already
unique on the wider key, because the narrower key it satisfied is a prefix of
it. `WITHOUT ROWID` is kept — the table is still a small keyed lookup and the
reason 001 gave for it has not changed.

`completed_at` and `read_count` come across unchanged and are now per edition.
That is what they always described: 001's comment calls them "finished once and
now re-reading", and a re-read is a re-read of a rendition.

**`progress` has no index to restore.** `005`'s note about `DROP TABLE` taking
a table's indexes with it is the reason to say so out loud;
`grep -n 'progress' schema/*.sql` finds no `CREATE INDEX`, the primary key
being the only access path.

**001's own comment on `progress` is not edited.** Migrations are append-only —
O45's reason, and separately the `sqlite_master` one — and 009 is where this
change is explained.

## 3. The store

`state::progress(conn, work_id, user_id)` returns **`Vec<Position>`** ordered by
`updated` descending, so the head is the most recent.

**`Position` is a new type and `Progress` is left alone**, which is a
distinction worth stating because collapsing them is the obvious move and it is
wrong. `Progress { edition, slot }` is the *write* shape: it is the body of
`PUT /progress` and what `apply_progress` deserialises out of a sibling
document. `Position { edition, slot, updated }` is the *read* shape. Giving
`Progress` an `updated` field would put a server-owned timestamp in a request
body, where a client would have to invent one and the server would have to
ignore it.

Three production callers, all named so none is missed:

| caller | what changes |
| --- | --- |
| `taiyaku-serve/src/progress.rs::load` | passes the vector through to the route |
| `migrate.rs::migrate_progress` | `is_some()` becomes `!is_empty()` |
| `export.rs::export_one` | serialises the set into the new document |

`state::put_progress` upserts on the wider key:
`ON CONFLICT(user_id, work_id, edition) DO UPDATE SET slot, updated`. It keeps
naming its columns rather than becoming an `INSERT OR REPLACE`, for the reason
its own doc comment gives, and
`put_progress_leaves_completed_at_and_read_count_alone` must stay red under
that swap.

New: **`state::move_progress`**, and its siblings for notes and truth, are §5.

## 4. The API and the document

### `GET /api/works/{work}/progress`

Answers the full set:

```json
{ "positions": [ { "edition": "raw", "slot": 33, "updated": "2026-08-27T09:24:41Z" } ] }
```

newest first. **204 when the reader has none**, unchanged — the route's doc
comment already explains why it parts company with `get_notes` there, and that
reasoning is untouched.

This is a **breaking change to the wire shape**, chosen over keeping
`{edition, slot}` and answering the most recent. The blast radius is one
consumer: `web/src/api.ts` has the only two call sites in the repository
(`GET` at :383, `PUT` at :408). The reader resumes into `positions[0]`.

`PUT` is unchanged on the wire — `{edition, slot}` — and now writes only that
edition's row.

`browse`'s `unread` carries over with its meaning intact: a work with no
`progress` row **for any edition**. `SeriesCard::unread`'s doc comment ("works
in this series with no `progress` row for the calling reader") needs one word,
not a rethink.

### `taiyaku.progress.json`

```json
{
  "taiyaku_progress": 1,
  "positions": [ { "edition": "raw-mono", "slot": 193 } ]
}
```

versioned the way `TruthDoc` is (`taiyaku_truth: 1`), because this document now
has a shape that can change again.

**A bare `{"edition": …, "slot": …}` is read as the legacy form** and converted
to a one-element list. This is not optional politeness: real documents are on
disk — `corpus/gotoubun-v01/taiyaku.progress.json` is
`{"edition": "raw-mono", "slot": 193}` — and `taiyaku serve <sidecar>…` runs
`migrate::apply_progress` over them at **every** start. A refusal there would
silently drop a reader's position and `export --in-place` would not write it
back.

Export writes the new shape only. The legacy read is pinned by a test over the
real corpus file rather than over a fixture: a fixture for this would be
written by the same hand as the reader of it, and would render right and wrong
identically.

## 5. The fold

### Detection

**An earlier draft of this section said "a work with zero `edition` rows is
emptied". That is wrong and the correction is worth keeping, because it is the
kind of mistake that reads as obviously right.** Under `Scope::Scan`,
`import::replace_editions` deletes *only the editions the incoming document
names*, and the emptied work is not written by this derive at all — it is not
in `groups`, so `write_work` never reaches it and `import` is never called for
it. The measurement confirms it from the other side: after the fold, works 3
and 4 still hold editions 3 and 4 over the same files. A work with zero
editions never occurs, so a trigger waiting for one never fires.

**What derive can actually see.** `write_work` resolves each group to a work
row; have it return that `work_id` and collect the set this derive wrote. A
**superseded** work is then one where all three hold:

1. this derive did not write it;
2. it holds at least one `member`, and **every** member's `sha256` is also held
   by a work this derive *did* write;
3. every one of its members has a non-NULL `root_id`.

(3) is what keeps an imported work safe. 006's own comment states that NULL
means "under no root", "which is exactly true of an imported work" — so a
hand-imported `taiyaku.json` that happens to name the same archive is never
superseded by a scan. A scanned work whose root was removed also has NULL
`root_id` throughout (`ON DELETE SET NULL`) and is likewise left alone, which
is the safe direction.

(2) is deliberately "every member", not "any": a work half of whose material
moved is not emptied, and deleting it would take material nothing else holds.

### Mapping, by member digest rather than by the rule

For each edition of the emptied work, find the edition on the **target** work
whose `member` rows carry the same `sha256` list in the same `ordinal` order.

The measurement is what makes this available: the emptied work's `raw` edition
and the target's new `full-color` edition hold byte-identical archives —
`920d44d8…`, 195 pages, one member each — so slots, page indices and box
coordinates transfer one for one with nothing to re-derive.

Digest matching beat reading `set_edition_key` off the rule that caused the
fold. It is independent of *why* the material moved, so it will hold for any
later feature that moves material between works; and where no such edition
exists it can refuse honestly rather than guessing an edition key.

### The move, in one transaction, in this order

1. **`progress`** — upsert onto `(user_id, target_work, mapped_edition)`. A
   conflict is impossible by construction, the target edition having just been
   created; it is guarded anyway, keeping the newer `updated`.
2. **`note`** — `UPDATE work_id, edition`. `note.id` is a ULID and globally
   unique, so nothing collides. **The `edition` rewrite is not optional**: the
   measurement showed that a `note` moved with `edition` left as `raw` is
   accepted by every constraint in the schema and lands a note about a colour
   page on the monochrome archive. That is the failure `truth_member`'s own
   schema comment calls the sharpest form of this problem, reached by the
   obvious one-line fix, and it is why this step is spelled out rather than
   left to the implementer.
3. **`truth_page`, then `truth_box`, then `truth_member`** — **deleted and
   reinserted, page rows first**, not updated. `truth_box`'s composite foreign
   key names `(work_id, edition, page)` with no `ON UPDATE`, so
   `UPDATE truth_page SET work_id = …` is refused — measured, both with and
   without the edition rewrite. The insertion order is `put_truth`'s own order,
   which the composite key exists to enforce. On a page the target already
   holds, the target's row wins and the collision is counted.
4. **`DELETE FROM work WHERE id = <emptied>`** — the first in production code
   outside `schema/tests.rs`. Safe only because 1–3 ran first in the same
   transaction; that ordering is the whole of the safety argument and belongs
   in a comment on the delete.

### Reporting

`DeriveReport` gains `emptied_works` and counts for what moved — positions,
notes, truth pages — so the scan's summary line says what happened rather than
only what was derived. The existing `kept_editions` is the model: a cost
counted out loud rather than a silence.

## 6. What this deliberately does not do

**The emptied `series` row is left standing.** `series` has no dependency on
`work` — `work.series_id` is `ON DELETE SET NULL` in the other direction — so
deleting the works leaves the series row with nothing in it. Measured: `browse`
already filters a series with no works out of `/api/library/series`, so it is
invisible on screen. Deleting it would cascade `series_title`, `series_tag`,
`credit`, `external_id` and `collection_item` — a second cascade question of
exactly the kind this design exists to answer carefully, and not this branch's.
It gets a count in the report, and an O-number if it ever surfaces.

**O74 is not built here.** Un-accept still reopens the question and leaves the
material where the last scan put it. This design changes what a *scan* does,
which is half of what O74 needs; the surface and the withdraw path are a later
PR's.

**`truth_*` stay library-wide with no `user_id`.** 001's comment on that
decision is unaffected by anything here.

**A fold where the file moves on disk is not detected at all, and that is
deliberate.** §5's detection condition (2), as it shipped rather than as first
drafted here, matches the emptied work's member against the target's covering
member on **`path` as well as `sha256`** — a review found that `sha256` alone
reads a byte-identical file sitting at an unrelated path anywhere else in the
library as proof of a fold that never happened, and the fix was to require the
same path too. The cost of that safety is symmetric: a genuine fold where the
archive was renamed or moved to a new location between scans — same bytes, a
different path — leaves the superseded work's members naming a path nothing
scanned now holds, so condition (2) never matches. The work is never
recognised as superseded and stands forever, visibly duplicated, rather than
being folded on a coincidence or silently losing whatever a reader had on it.
Standing forever is the safe side of that trade, the same side §5's own
mapping step takes when no counterpart edition exists, and it is written down
here so a later reader finds a documented choice rather than what looks like a
bug nobody noticed.

**§0's five — `progress`, `note`, `truth_page`, `truth_box` and
`truth_member` — is what the measurement's *reader state* touched, not the
count of everything a `DELETE FROM work` reaches.** Re-derived directly from
the schema rather than recalled, **fifteen tables cascade from `work(id)`**:
eleven directly — `alignment`, `collection_item`, `credit`, `edition`,
`migrated_document`, `note`, `progress`, `truth_box`, `truth_member`,
`truth_page` and `work_tag` — and four more transitively: `member` and
`page_flag` through `edition`, `run` through `alignment`, and `work_fts`
through the `work_ad` trigger that removes a deleted work's own row from its
full-text mirror on the way out. Only three of the eleven direct tables hold a
reader's own state; `fold::absorb` moves those, and counts the rest — the
library's own description of the material, not a reader's — as
[`Moved::discarded`] rather than losing them without a number attached.

## 7. Testing

| what | why it is the test |
| --- | --- |
| migration over rows on two editions of one work | the copy must not merge them; the narrow key made this shape unreachable before |
| `put_progress` leaves a sibling edition's row standing | red before the change, green after — the behaviour change stated in §1 |
| `put_progress_leaves_completed_at_and_read_count_alone` | must still be red under an `INSERT OR REPLACE` swap after the key widens |
| legacy document read, over `corpus/gotoubun-v01/taiyaku.progress.json` | a real file, not a fixture |
| `GET /progress` answers 204 with no rows and a newest-first set with several | the contract two clients read |
| the fold, end to end on the four-archive tree | accept, rescan, then assert **2 works, 4 editions, 4 members over 4 files**, and the reader's colour position on work 1's `full-color` edition at slot 33 |
| a `note` whose edition is not rewritten | the failure §5 step 2 exists to prevent, asserted rather than trusted |
| mutation sweep over the move and the digest mapping | per `.cargo/mutants.toml`; the mapping is where a silent wrong answer would live |

The end-to-end test needs the corpus and belongs with the standing corpus
checks, which do not run in CI. The rest run in the ordinary suite.

## 8. Sequence

One PR, one claim: **the library stops holding a folded volume twice, and a
reader keeps their place in every edition of it.** In order — 009 and the store
(§2, §3), then the document and the API with its one client (§4), then the fold
(§5). Each step leaves the suite green; the fold is last because it is the only
one that needs all of the others.

# Reconciling hand documents with the works that own them — design

**Status: settled in brainstorming on 2026-08-31. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This design answers **O81** and **O82** together, because the measurement they
were waiting on showed they are one problem seen from two ends. That
measurement was taken on 2026-08-31 and is recorded in
[`../../notes.md`](../../notes.md), under *"O81 and O82, measured against real
archives — 2026-08-31"*. Every figure below comes from it rather than from
reading the code.

## 0. What the measurement settled, in one paragraph

Five works were built from real archives in one root, each given a hand-set
title through `PATCH /api/library/works/{slug}`, and each subjected to one
ordinary event. Repacking an archive and adding a page to one both moved the
work's `content_key`, so `hand::path_for` stopped addressing the document that
recorded the correction; a rename moved nothing, exactly as O81's entry
claims. An edit made while the root was read-only answered `persisted: false`
honestly and left the *existing* document in place holding the previous
correction. Rebuilding the database from the same root then cost **three of the
five works their correction**: two came back at their derived filename, and
the third came back at its older title, silently, over the newer one the live
database held. That is store spec §4.4 — *losing the database costs a rescan,
not the organisational work* — failing on the two most routine events a
library sees.

Two of O81's and O82's own claims did not survive contact with this:

- **O81 said derivation re-derives over the work.** It does not, and has not
  since O80 closed. Migration 011's `_hand` columns are re-applied by
  `COALESCE` on every pass, and `title_hand` came through all three
  key-moving events intact. What is lost is durability alone.
- **O82 described a work whose document is absent.** The read-only case
  produces a third state it does not name: a document that is *present and
  disagrees*. A sweep counting only presence calls that work reachable and
  walks past it.

## 1. The fact that shapes everything

**An orphaned document cannot be re-adopted.** Its filename is the old
`content_key`, which names content that no longer exists, and since O80 closed
(#97, *"the document projects only what a person set"*) the document carries
nothing else identifying. A title-only edit — the common case, and the one the
metadata editor's single control produces — projects exactly this:

```json
{
  "taiyaku_library": 1,
  "updated": "2026-08-31T07:33:04Z",
  "title": "HAND go-toubun-no-hanayome---v01-cbz"
}
```

No series, no volume, no member digests. Nothing to match a work by.

This was verified against the five documents the measurement left on disk, not
reasoned about, and it is worth stating because it inverts the obvious build
order. **The rename at rekey time is not the cheaper of two fixes — it is the
only moment the link between file and work is knowable.** After
`import::scanned`'s transaction commits, the old key is gone from the database
and the file is unattributable except by a person opening it and reading the
title.

**"Both keys are in hand for exactly the length of one `import_with` call" was
wrong, and it is corrected where the code it describes actually runs (§5, §9).**
The rename does not happen inside that call at all: it runs from
`derive::write_work`, after `write_roots`, because `member.root_id` is still
NULL immediately after `import_with` returns for a single-member work's
repack, and `path_for` cannot resolve an address without it. The old key
survives past the commit regardless, carried in a local (`rekeyed_from`)
rather than read back from the row, so it is still in hand once `write_roots`
makes the new address resolvable. The larger point this paragraph makes is
unaffected: an orphaned document cannot be re-adopted, so the rename has to
happen at whichever single moment both keys are knowable — only *where* that
moment falls moved, not the fact that there is exactly one.

It also fixes what the reporting half owes: an orphan report that names only a
path is useless, because the path is a hash. It must carry the **title the
document holds**, since that is the only thing a person can act on.

## 2. The decision: repair what is unambiguous, report what is not

Three states, three answers:

| state | what it is | answer |
| ----- | ---------- | ------ |
| **gap** | a marked work with no file at its address | **repaired** — re-project and write |
| **orphan** | a file no work addresses | **reported** — path and the title it holds |
| **stale** | a file present that disagrees with the database | **reported** — touched by nothing |

**Only the stale case is a genuine conflict**, and it is the one left
unresolved. Gap and orphan each have exactly one correct answer; stale has two
candidates and nothing to choose between them.

**Three alternatives for the stale case were considered and lost.**

*The database wins — rewrite the file.* Cheapest, and it would fold gap and
stale into one code path. Rejected because O44 records that
`taiyaku.library.json` is **the one document this project designs for
hand-editing**, so this silently discards an edit made directly to the file — a
designed use. Nothing measures how often that happens, so shipping this rule
would be shipping a resolution on plausibility, which is what O1 established
against.

*Newest wins, by timestamp.* The correct long-term answer, and it handles both
directions. Rejected for now on cost and on evidence: it needs a migration,
because **the database has no timestamp to compare the document's `updated`
against**. `record_absorbed`'s upsert is `DO UPDATE SET absorbed =
excluded.absorbed` and never touches `migrated` — verified by the measurement,
where work 5's marker still read `07:33:04` after an edit at `07:35:35`. It
also asks two clocks to agree across a NAS write from another host and an
mtime that may not survive a copy.

*Keep today's read-back semantics and never overwrite an existing file.* The
narrowest change, and it leaves the measured revert exactly as it is.

**Why reporting is the right answer today, and not merely the timid one.** The
stale state exists because a write failed. Once §4's retry lands, the window
narrows to *"a write failed **and** somebody hand-edited that file before the
retry ran"*. Building an automatic resolver now is building for a case the rest
of this design is about to make rare. If the reporting shows staleness happens
for real, that is the evidence that earns the timestamp column.

## 3. `hand::reconcile`

One function, in `hand.rs`, beside the address arithmetic it depends on:

```rust
pub fn reconcile(conn: &Connection, root: &Root) -> Result<Reconciled, String>
```

One `read_dir` per root, joined against the register of works that owe a
document:

```sql
SELECT work_id FROM migrated_document
 WHERE document = 'library' AND absorbed = 1
```

resolved through `path_for`. `Reconciled` carries the three states, and its
field shapes follow `DeriveReport::unreadable_hand`'s precedent — *"a `Vec`
rather than a counter, because the person has to be told which file to fix"*:

- `repaired: usize` — gaps written on this pass
- `unwritable: Vec<String>` — gaps that could not be, each with the
  `io::Error`'s own message naming the path, in `hand::save`'s existing shape
- `orphans: Vec<Orphan>` — path **and** the title the document holds
- `stale: Vec<String>` — path, for a file that disagrees with what
  `librarydoc::of` projects

**`orphans` carries a struct rather than a formatted string** because the
filename is a hash and the title is the only actionable half; formatting them
into one line at the report site would make the CLI and any future screen
re-parse what this already knows.

**Staleness is decided by comparing projections, not bytes.** `librarydoc::of`
against the file's parsed `LibraryDoc`, so key order and whitespace do not
manufacture a disagreement. A file that fails to read or parse is *not* a
stale document — it is already `unreadable_hand`'s, which exists and is
unchanged by this.

## 4. The gap retry

A marked work whose address holds no file is re-projected and written, through
`hand::save`'s existing path rather than a second writer. This is what makes
O82's *"nothing ever retries it"* false: the next scan after a root becomes
writable carries the correction to disk.

**A permanently read-only root must not degrade into noise.** Every scan will
attempt and fail, so the failure is *counted and named*, never raised — the
same decision `hand::save` already makes in answering `persisted: false`
rather than an error. A library on read-only media reports the same
`unwritable` list every scan, which is honest: the condition really is still
true.

## 5. `hand::rename_for_rekey`

Prevention, at the one moment the link exists.

**This section originally placed the call in `import_with`, right after
`tx.commit()`. That placement cannot work — the paragraph below explains why,
and `derive.rs`'s own comment at the actual call site has the fullest
account.**

`import_with`'s probe resolves `existing` from content first and from `known`
— the `(series_id, volume)` address — on a miss. **A hit on the second is
exactly a rekey**: the row's current `content_key` is still readable, and
`key` is the one about to be written over it. `import::scanned` is the caller
that matters; the document-backed path addresses its document by
`sidecar_path` and is out of scope (§7).

**The call actually sits in `derive::write_work`, immediately after
`write_roots`.** `hand::path_for` resolves a work's address through
`member.root_id`, and for a single-member work a repack changes that member's
`sha256` — so `import::replace_editions`'s delete-and-reinsert leaves
`root_id` NULL there, because `held_member_meta` restores it keyed on
`sha256`, exactly what changed. Calling the rename inside `import_with`, as
this section first proposed, finds `path_for` answering `None` and leaves the
document unmoved. `write_roots` restores `root_id` by `(edition key,
ordinal)` instead and runs after `import::scanned` returns, so
`derive::write_work` is the earliest point at which the rename can resolve an
address at all.

Two rules, both load-bearing:

**The rename runs after `import::scanned`'s transaction has committed, never
inside it.** A rollback that left the file renamed would strand it under a
key the database never adopted — this defect, inverted, and harder to see
because the report would show everything reconciled. The old key travels past
the commit, in `rekeyed_from`, to be applied once `write_roots` has run.

**An existing file at the target address is never overwritten.** Two works
cannot legitimately share an address, so a collision means something this
design did not predict; it is reported as an orphan rather than resolved, and
the correction under the old key survives for a person to read.

## 6. Where it runs

Inside `derive()`, which already walks every work and already calls
`hand::load` per work, and `DeriveReport` is where every other outcome of
this shape is already reported — `suppressed`, `rekeyed`, `kept_editions`,
`unreadable_hand` — under the rule those fields were built on: **any
derivation outcome that changes what a person sees and cannot be inferred
from the other counts gets a field.**

**"One `read_dir` per root is nothing beside hashing archives" understated
the cost, and there is now a measurement to correct it with.** `reconcile`
pays one `read_dir` per root, **plus** `hand::path_for` for every marked
work — the `owed` query that finds marked works is global rather than
root-scoped, so this runs once per root per marked work, i.e. roots × works
point lookups, not works alone — **plus** a file read, a JSON parse and a
full `librarydoc::of` projection for every present, addressed document (the
stale check). All of it is paid unconditionally on every `derive`, unlike
the read-back beside it, which `!absorbed` gates. It is not an N+1 inside an
N+1 the way O85 was — roots × works point lookups against SQLite, not a
query nested inside a query — and measured rather than recalled: **2000
works across 4 roots cost 105 ms; 5000 works across 10 roots cost 573 ms**
(release build, in-memory database, tmpfs; see `notes.md`'s "O81 and O82,
measured against real archives — 2026-08-31", subsection "Reconciliation's
own cost, measured"). On a NAS the file reads, not the SQL, are expected to
dominate instead.

**One ordering constraint, and the code already documents it.** `derive.rs`'s
read-back records that it must run after `write_roots`, or `member.root_id` is
still NULL when `path_for` runs, the `JOIN root` matches nothing, `path_for`
answers `None`, and *"a correction sitting right beside the archives is
silently skipped"* — the store spec §4.4 claim broken in the other direction,
costing two rescans instead of one. `reconcile` resolves addresses the same
way and has the identical dependency. It sits after `write_roots`, in the same
place and for the same reason.

## 7. What is out of scope, stated rather than dropped

**Sidecar-backed works.** `path_for` answers `None` for them by design: their
document lives beside the sidecar and `export::work` writes it. Their
`persisted: false` means *"there is no address at all"*, which is a different
condition from a write that failed, and `metadata.rs` already documents that
asymmetry. Reconciling `<stem>.library.json` beside a sidecar is a second
design with a different register.

**Automatic stale resolution.** Decided in §2.

**Deleting an orphan.** Never, on any schedule, for any reason. It is somebody's
organisational work, and this whole module exists to stop derivation walking
over exactly that.

## 8. Testing

The rule from every fix round on this project: a fixture where right and wrong
render the same proves nothing, so each test below is one that **fails when
the code it covers is mutated**, hand-applied and watched to fail before it is
trusted.

1. **Repack and rescan.** Assert the document moved to the new key's address.
   Fails if `rename_for_rekey` is dropped.
2. **Read-only root, then writable.** Assert the next derive writes the
   document. Fails if the retry is dropped.
3. **Orphan report.** Assert it names the file **and** the title inside it —
   the second half is the one a mutation that returns paths alone would
   otherwise survive.
4. **Stale report.** Assert a document that disagrees is named and **not
   rewritten**, which is the assertion that pins §2's decision in code rather
   than only in this file.
5. **The real one: rebuild the database and assert the correction survives.**
   Delete the database, rescan the same root, and check the hand-set title is
   back. This is the store spec §4.4 claim stated as a test, and it is the
   check that would have caught all of this originally — the existing suite
   could not, because it never rebuilt.

Test 5 is also the one to run against real archives before either PR merges,
in the second mode `docs/notes.md` records: a scratch root of hardlinked or
copied volumes, driven through the API rather than through `INSERT`. On #91
that mode found a merge blocker that ten per-task reviews, a whole-branch
review, the full suite and a green gate had all passed.

## 9. Sequencing

**PR 1 — the safety net.** `hand::reconcile`, the four `DeriveReport` fields,
the gap retry, and the CLI's scan summary line. This lands first because it
makes every existing stranded document visible, including the ones a library
already holds today, and because it is the half that does not touch the import
path.

**PR 2 — prevention.** `rename_for_rekey`, called from `derive::write_work`
after `write_roots` (§5 records why that moved from the `import_with`
placement this once said). Smaller, and it lands second so that if the rename
is wrong the reporting is already there to say so.

Both are `taiyaku-library` only. Neither adds a route, a screen or a CLI
subcommand: the sweep runs where the walk already runs, and its results print
where the walk's results already print.

## 10. What this does not fix

**The correction is still only as durable as the root is writable.** A library
whose media is read-only keeps its corrections in the database and reports so
on every scan. That is the honest end state and not a defect — the alternative
is refusing the edit, which O82's entry already rejected.

**A stale document still reverts a newer edit on a database rebuild**, until
the timestamp question is answered. §2 says what would earn that column: the
reporting this design ships, showing the state actually occurs.

**Files that change while no database is watching strand their documents
unrecoverably.** §5's rename needs a scan to observe the transition, because
that scan is the only thing that ever holds the old key and the new one at the
same time. Repack a volume with the library's database deleted, and the rebuilt
database computes the new `content_key` while the file on disk is still named
for the old bytes — no rename can reach it, and §1's fact says nothing else
can identify it either. The reconciliation reports it as an orphan with its
title, which is the whole of what is possible. This is a real hole and it is
narrow: it needs the database to be absent *and* the archives to change before
it is next present, which is a restore-from-backup or a move between machines,
not an ordinary week.

**Neither item closes on this design alone.** O81 and O82 are one
reconciliation, and whether they become one row in `docs/status.md` is a
decision about the list rather than about the code — it moves the counts, so
it is not made here.

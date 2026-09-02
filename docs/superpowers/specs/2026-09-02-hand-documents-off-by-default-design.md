# Hand documents off by default, and configurable per root — design

**Status: settled in brainstorming on 2026-09-02. Built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This narrows a claim rather than fixing a defect. It follows the
reconciliation work of 2026-08-31/09-01, designed in
[the reconciliation spec](2026-08-31-hand-document-reconciliation-design.md),
which made a stranded `<root>/.taiyaku/<content_key>.library.json` visible and
then stopped it stranding. What nobody asked until now is whether that file
should be in the library at all.

## 0. The question, and the answer

`<root>/.taiyaku/` appears in a person's media tree without their asking. Its
whole purpose is store spec §4.4's claim — *losing the database costs a rescan
rather than costing the organizational work* — and it earns that purpose in
exactly one circumstance: the database is lost or rebuilt.

**The document is a projection, not a source.** `hand::save` writes what
`librarydoc::of` reads out of the `_hand` columns the database already holds,
and nothing reads it back while the `LIBRARY` marker is armed. In normal
operation it is written and never consulted.

So it is insurance, and on a deployment whose database lives in a backed-up
application-data directory alongside every other service, that insurance is
already bought. **Documents are therefore off by default**, and a root that
wants them says so.

## 1. The setting lives on `root`

```sql
ALTER TABLE root ADD COLUMN keeps_documents INTEGER NOT NULL DEFAULT 0;
```

Migration 012. `0` is off, and off is the default for every existing row.

**A per-root column beat a library-wide settings table on cost, which is the
reverse of what it looks like.** `root` already carries `edition_key`,
`default_kind`, `default_language`, `exclude_globs` and `scan_interval_secs` —
it is this project's configuration table already — and `hand::path_for`
already reads that row to resolve an address, so the value arrives where it is
needed with no new query. A `setting` table would have been new schema, a new
accessor and a new cache, to hold strictly less.

It is also the more useful granularity: a library spanning a NAS mount and a
portable disk can keep documents on the disk that travels and not on the one
that is backed up. That flexibility is a side effect of the cheaper choice
rather than a reason for it.

**Spec C4 still owns configuration as a screen.** This is one column with a
default, not a settings system; C4 gives it a control when it gives `root`'s
other columns theirs.

## 2. What "off" means, operation by operation

**`hand::save` writes nothing and reports success.** Nothing was owed and
nothing is missing, so `persisted` is `true` and `why` is `None`. See §3 for
how a caller tells this apart from a write that succeeded.

**The `LIBRARY` marker is still armed.** It means *this work's document has
been absorbed*, which stays true when there was never one to absorb, and
`derive::write_work`'s read-back guard reads it before deciding whether to
call `hand::load`. Leaving it unarmed would make every derive call `load` for
every work forever, finding nothing each time — a per-work query bought for no
answer. The alternative considered was making the marker mean *a file exists*,
which is a second meaning for a field that O80 already had to strip a second
meaning from.

**`reconcile` must not repair in an off root — though the risk this guards
against is narrower than first reasoned here.** A marked work whose address
holds no file is a *gap*, and repairing a gap writes the file. **This
paragraph originally called the branch "the whole risk of the change" and
warned that without it "the next scan would recreate every document the
setting was meant to prevent" — wrong once `hand::save` gained its own guard,
above in this section: `save` already refuses the write for a declined root,
so deleting this branch alone hands every gap to `save`, which still writes
nothing.** A hand-applied mutation during implementation confirmed it:
deleting the branch reported `repaired: 1` with no `.taiyaku` directory
created. What the branch actually prevents is `reconcile` claiming a repair
that never happened — the report going false, not the disk. The two guards are
independent refusals, and the real hazard is relaxing one while believing the
other is the backstop.

**`reconcile` still reports orphans in an off root — but only the documents
nothing addresses at all, which this paragraph originally did not say and
which was a Critical, found in review.** A work stays marked `absorbed = 1`
after its root's `keeps_documents` turns off, so an off root's *addressed*
files still matched `expected` too; an earlier `reconcile` gated the whole
comparison branch on `root.keeps_documents` rather than branching on that
match, so every one of those addressed files fell through to the orphan
report regardless. Migration 012 defaults every existing root off, so the
first derive after upgrading called *every* hand document in the library
stranded — on the same pass `hand::load` had just read each one's title back
into the database, since `hand::load` is deliberately not gated on
`keeps_documents` at all — a rebuilt database must still be able to read a
document back off disk. The fix branches on
`expected.get(&path)` instead of gating the lookup on it, and the
addressed-but-declined case gets its own name, `hand::Declined`, rather than
borrowing the orphan's. A document left by an earlier setting, by another
install, or by a library moved between machines is inert but real, and a
person should be told it is there rather than finding it years later.
**Nothing is ever deleted** — the rule this whole module was built on does
not bend for a setting.

**`rename_for_rekey` and the fold provenance need no change.** Both already
answer "nothing to do" when there is no file at the address, so an off root
takes their existing empty path rather than a new one.

## 3. The response gains a third state

`PATCH /api/library/works/{slug}` answers `persisted: false` with a `why` when
a document does not reach disk, and `MetadataEditor` renders a note saying the
edit is safe but not durable. With documents off, every save would carry that
note, turning a real signal into permanent furniture.

So the response distinguishes three outcomes, not two:

| outcome | `persisted` | what the client says |
| ------- | ----------- | -------------------- |
| the document was written | `true` | nothing |
| this root keeps no documents | `true`, with the state named | nothing |
| a write was attempted and failed | `false`, with `why` | the existing note |

**`persisted: true` for a root that keeps no documents, rather than `false`.**
The field answers *did this save do everything it was supposed to*, and it
did. The alternative — keeping `persisted` strictly as *is there a file on
disk*, which stays literally true — was rejected because it makes the server
report a fact that reads as a warning, and obliges every future client to know
not to alarm at it. Naming the state explicitly costs one field and one branch
in the editor, and keeps `persisted: false` meaning something.

**The names, pinned so they are not invented twice.** The JSON field is
`keeps_documents: false`, present only when a root keeps none — absent means
the ordinary case, matching how `why` is absent unless there is one.
Internally `hand::Saved` gains `keeps_documents: Option<bool>` beside `path`,
`persisted` and `why` — **three states, not two**, which planning corrected
from an earlier `bool` in this document. `Some(false)` is *this root declined
documents*; `Some(true)` is *it keeps them and one was written*; `None` is
*there was no root to ask*, the imported work whose document belongs beside
its sidecar. A plain `bool` would have made the third case indistinguishable
from the first, which is the exact conflation the paragraph below is about.

That internal field is what stops a third meaning collecting on `path: None`.
It already carries two: *this work has no address* — an imported work whose
document belongs beside its sidecar, an asymmetry `metadata.rs` documents —
and, without this, *this root keeps no documents*. Two causes behind one
`None` is the shape O90's `unreadable` counter is filed under, and one
instance of it is enough.

## 4. What stays out

**`<stem>.library.json` beside a sidecar is untouched.** It is part of the
sidecar format (§4.4 proper), written by `export::work` when a person asks for
an export, and it lands beside a `taiyaku.json` they already have. It is not
littering: it was requested. `hand.rs`'s `<root>/.taiyaku/` address exists
because `import::scanned` leaves `sidecar_path` NULL and a scanned work had
nowhere else to keep what nothing can re-derive — that is the file this design
turns off.

**No deletion of existing documents, and no migration of them anywhere.**
Turning the setting off leaves what is on disk exactly where it is.

**No change to `import`, `export`, or the sidecar round trip.**

**A command turns `keeps_documents` back on. This paragraph used to argue
against one, and the argument lost.** It read that no CLI flag was deliberate
rather than an oversight, on the reasoning that spec C4 owns configuration as
a screen (§1 above) and a one-off flag here would be a second, throwaway
answer to a question C4 already owns.

What that reasoning missed is who it was leaving stranded. Migration 012's own
comment, §4.4's qualification below and O82's row all say plainly that the
default narrowed — but they tell a *reader of the docs*, not a person whose
library just stopped writing documents it used to write. A whole-branch review
on 2026-09-02 put it as the question the design had not asked: the upgrade is
honest in the docs and silent in the product. Leaving the only remedy as raw
SQL against the library database is not a deferral, it is a dead end for
anyone who does not read this file.

So the flag ships, and C4 still owns the screen:

```
taiyaku library root documents <PATH> <on|off> [--db PATH]
```

It changes one column and nothing else — no scan, no derive, and no document
created, moved or deleted. A root turned on gets its documents written by the
next scan's `hand::reconcile`, which is already the mechanism for exactly
that, and the command's own output says so, because otherwise the reasonable
expectation is that files appear at once.

The equivalent SQL, for anyone reaching the column another way:

```sql
UPDATE root SET keeps_documents = 1 WHERE path = '/path/to/the/root';
```

`root.path` holds the canonical path `scan::add_root` resolved when the root
was added, not necessarily the string typed at `--add` — `select_roots`
canonicalizes `--root` the same way before matching, for the same reason. The
safest way to get the exact stored value is to read it back first:
`SELECT path FROM root`. Nothing here is destructive:
flipping the column back on does not write, move, or delete anything by
itself, and `hand::save` starts writing again only from that point forward,
same as any other root that has always kept documents.

## 5. What this costs, stated rather than discovered

**Store spec §4.4's claim narrows and the spec must say so.** *Losing the
database costs a rescan, not the organisational work* becomes true only of a
root that keeps documents. For every other root, losing the database without a
backup loses the corrections. That is a real reduction in what the project
promises, chosen deliberately, and §4.4 should carry the qualification rather
than being left to read as unconditional.

**Most of the reconciliation work becomes dormant by default.**
`hand::reconcile`, `rename_for_rekey` and the fold provenance stay correct and
tested, but a default install exercises none of them. They are not deleted:
the mechanism is a supported configuration, and the measurement that would
decide the two questions still open against it — whether a fold should carry a
document, and whether a stale one should be resolved — can only ever come from
a deployment that has them on.

**A `persisted: false` a person has already seen may become `true` after this
lands**, for the same work and the same edit, because the root now keeps no
documents. That is correct and still worth knowing when reading old reports.

## 6. Testing

Each of these fails under a plausible mutation, which is the bar this project
holds a fixture to:

1. **An off root writes no document.** Hand-set a title, assert the address
   holds no file and `.taiyaku` was never created.
2. **The repair branch, which is the one that matters.** An off root holding a
   marked work with no file: assert `reconcile` reports `repaired == 0` and
   that no file appears. Hand-apply the removal of the off-branch and confirm
   the test fails — without that check this test proves nothing, since the
   pass state and the bug state differ only by a file nobody looked for.
3. **An on root still writes**, so the setting is a setting and not a removal.
4. **An off root still reports an orphan** left on disk from before — and,
   distinctly, reports a document a live work still addresses as *declined*
   rather than folding it into the same list. The two were conflated once
   (§2's amendment above records it as a Critical) and the fixture that
   caught it hand-applies the fold back in to confirm the test goes red.
5. **The response's third state** — a save against an off root answers
   `persisted: true` and names the state, and against an unwritable on root
   still answers `persisted: false` with a `why`.
6. **The default is off**: a root added by `scan --add` keeps no documents
   unless told to.

## 7. Sequencing

One PR. It is a migration, a branch in `hand::save`, a branch in
`reconcile`, a field on `Saved` and on the PATCH response, one editor branch,
and the §4.4 qualification. Stacked behind #103, which is behind #102, #101
and #100 — this design's own subject is the code those three PRs add, so it
cannot sensibly land before them.

## 8. What this does not decide

**Whether the address should be content-derived at all.** The deeper question
— that `<content_key>.library.json` moves whenever the content moves, which is
the origin of O81, O82 and every rename in #101 — is untouched here. Turning
the file off by default reduces how much that costs without answering it. If
it is ever answered, a stored address on `work` would dissolve the rename, the
fold-carry question and the offline-mutation hole together, at the price of
making path authoritative for the document against §4.1's *identity is
content, path is a hint*.

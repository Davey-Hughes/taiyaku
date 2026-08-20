# The library store — design

**Status: settled in brainstorming on 2026-08-14. Built, and shipped on
`library-store`.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is the first of several specs. It designs **the store alone** — a SQLite
database, a crate to hold it, and the round trip between it and the sidecar
documents that exist today. It designs neither the scanner that will fill it,
nor the authentication that will guard it, nor the browse UI that is the point
of the whole exercise. §10 says where each of those goes and why the seam is
there, and §13 designs five pieces of Komga and Stump parity that an audit found
nobody had named — download, metadata editing, read status, backup, and per-root
settings — because four of them turned out to touch migration 1 and one of them
turned out to write to your library.

## Where it came from

The ask was "a library side of this project, like what komga and stump
provide", for two stated reasons: it is the biggest single win available, and
it would make manually testing everything else enjoyable. Two later messages
sharpened it into something more specific than a Komga clone:

> I want better organization than what komga and stump provide. it is annoying
> that on these you can't group editions together, and basically the other
> things that taiyaku originally set out to figure out.

> I am planning on hosting this as a replacement for my komga and stump hosts,
> so I want full auth capability

So this is not a front door bolted onto the aligner. It is the aligner's model
— a work with several editions — offered as the library primitive that neither
Komga nor Stump has.

## 1. What is actually missing today, measured rather than asserted

**`taiyaku serve` cannot serve the corpus.** `work_id` derives a work's id from
its sidecar's **filename** (`works.rs:164`), and `taiyaku.json` beside each work
is the convention this project documents (`sidecar.md`). Every corpus sidecar
therefore slugifies to `taiyaku`, and `load_works` refuses the collision rather
than shadowing one silently — deliberately, and tested
(`works/tests.rs:26`, `two_sidecars_claiming_one_work_id_are_refused_rather_than_shadowed`).

That is not a bug. It is the correct behaviour of a design that never intended
to hold more than one work, and `docs/status.md` says so in as many words:

> `serve` is handed sidecar paths the same way `align` is handed members,
> because deciding which files constitute a library is a different job

**This spec takes that job on.** That is a reversal of a stated position rather
than the filling of a gap, and it is recorded as one.

Downstream of it, the browser reader opens `works()[0]` (`startup.ts:113`,
`openFirstWork`) — which is not a shortcut, it is the only correct thing to do
against a server that can only ever hold one work.

**What exists and is reusable, so the scope below is smaller than it looks:**

| Already built | Where | Used here for |
| --- | --- | --- |
| Sidecar parse/serialise, deterministic | `taiyaku-core/src/sidecar.rs` | the projection, and the round-trip test |
| `Sidecar::merge`, carrying hand runs | `core/sidecar.rs:418` (`Merged`) | the whole reconciliation rule (§4) |
| `Sidecar::correct`, typed refusals | `core/sidecar.rs:437` (`Corrected`/`NotCorrected`) | unchanged; the PATCH route keeps working |
| Cover and thumbnail generation, content-addressed | `taiyaku-serve/src/cover.rs` | unchanged |
| Stump resolution and the composite page source | `taiyaku-serve/src/stump/`, `composite.rs` | unchanged |
| OPDS 1.2 + PSE feed grouping editions | `taiyaku-serve/src/opds.rs` | unchanged in A; gains auth in A2 |
| ComicInfo.xml reading | `taiyaku-cli/src/credits.rs:663` | metadata at scan time (spec B) |
| Volume number from a name, incl. `第077巻` | `taiyaku-cli/src/inventory.rs:35` | volume numbers at scan time (spec B) |
| Atomic write | `taiyaku-serve/src/atomic.rs` | export; moves down a crate, §7 |

## 2. The model: four levels, and no special case for "aligned"

```
series ──▶ work (a volume) ──▶ edition ──▶ member (an archive or page directory)
```

A **Komga book is a work with one edition and one member, and no alignment.**
An aligned pair is a work with two editions and one alignment. Nothing in the
schema distinguishes them, there is no `is_aligned` flag, and no query needs to
branch on one.

This is the decision the whole design rests on, and the alternative it beat was
a two-object model — `book` for ordinary comics and `work` for aligned ones —
which was rejected because every route, every browse query and every progress
row would then have to handle both, and the interesting case (a book that later
*gains* a second edition) would be a type change rather than an insert.

**It is also the answer to "better organization than Komga and Stump".** Those
two have one book per file. A raw and its translation are two books with no
relation expressible between them; a scanlation group and a publisher compete
for the same metadata field. Here they are two editions of one work, and §3's
`credit` table attaches a person or a group to the **edition** rather than to
the work.

## 3. The schema

Migration 1, in full. `journal_mode = WAL` is a property of the database and is
set once; **`PRAGMA foreign_keys = ON` is a property of the connection** and
must be set on every one this crate opens, including the ones tests open. It is
off by default in SQLite for backward compatibility, so a `Library::open` that
forgets it enforces nothing and every `REFERENCES` below becomes a comment.

### 3.1 Structure

```sql
series (id, sort_title, reading, cover_work_id NULL)
    -- cover_work_id references work(id), which references series(id).
    -- The cycle is fine and is why the column is nullable: a series is
    -- inserted with no cover, and the work that becomes its cover is
    -- inserted second.

-- one series, several names: 「五等分の花嫁」, Go-Toubun no Hanayome,
-- The Quintessential Quintuplets. Komga's single title is the annoyance.
series_title (series_id, title, kind, is_primary,  PK(series_id, title))
    -- kind: 'romaji' | 'native' | 'english' | 'alt'

work (id, content_key UNIQUE, slug UNIQUE, series_id, volume REAL,
      title, reading, hub, sidecar_path, sidecar_mtime, imported)

edition (id, work_id, key, label, language, kind,  UNIQUE(work_id, key))
    -- kind: 'raw' | 'official' | 'fan' | NULL

member (id, edition_id, ordinal, path, sha256, pages, format,
        chapter_declared REAL, chapter_source,  UNIQUE(edition_id, ordinal))
    -- format: 'zip' | 'dir' | 'epub-fixed' | 'epub-reflow' | 'pdf' | 'rar' | '7z'
    -- chapter_source: 'comicinfo' | 'filename' | 'hand'

page_flag (edition_id, kind, page,  PK(edition_id, kind, page)) WITHOUT ROWID
    -- kind: 'spread' | 'credit'
```

`member.ordinal` is not decoration. `sidecar.md`'s "How a member becomes pages"
is **normative**: an edition's page sequence is the concatenation of its members
*in the order given*, and a consumer that reorders them disagrees with every run
in the document while `sha256` and `pages` still match. The `UNIQUE(edition_id,
ordinal)` is what makes that unrepresentable rather than merely documented.

`volume` is `REAL` rather than `INTEGER`. Omnibus editions and half-volumes
exist; `inventory::volume_number` returning `u32` is a limit of that parser
rather than a fact about books.

### 3.2 Alignment

```sql
alignment (id, work_id, from_key, to_key, by, solver_hash, solver_floor,
           verdict, slots, paired, avg_run)

run (id, alignment_id, ordinal,
     a_start_page, a_start_half, a_end_page, a_end_half,
     b_start_page, b_start_half, b_end_page, b_end_half,
     margin, by, note,  UNIQUE(alignment_id, ordinal))
```

**No `UNIQUE(work_id, from_key, to_key)` — this section carried one until
whole-branch review 2 (I3), and it was a rule this store had no business
making.** Nothing in the format forbids two alignments for one edition pair:
`Sidecar::from_json` is a plain serde parse, every edge lookup in
`core/sidecar.rs` resolves with `.find(…)`/`.position(…)` — so a repeated pair
has always meant "the first one" — and `serve` read exactly that meaning off
the file before this store existed. The constraint refused a legal document at
import with a raw SQLite message, and because `serve` imports every path it is
handed before serving any, one hand-edited work aborted the whole server. Both
rows are kept now, read back `ORDER BY id`, so insertion order is document
order and the projection means what the file meant. Deciding *between* two
edges is `taiyaku-core`'s job; §4.2's rule applies here too. `page_flag`'s
primary key stays, since it is a true statement about the rows, but `import.rs`
writes it `OR IGNORE`: `"spreads": [3, 3]` states one fact twice, and that list
has always been a set (`ORDER BY page` on the way back).

**Runs are normalised into rows rather than stored as the sidecar's JSON blob.**
The alternative — one `runs TEXT` column holding the document's own array — was
rejected on two counts: hand correction already operates on a single run
(`PATCH /api/works/{work}/alignments`), and §4's reapplication has to match a
correction to a run by its span. Both would become a read-modify-write of the
whole document under a blob.

**Slot spans store four columns per side rather than the sidecar's `"1-4R"`
string.** A `Slot` is `{ page: u32, half: Option<Half> }` (`slot.rs:44`), and
storing the rendered string would make "which runs cover page 47" a text scan.
Round-tripping goes through `SlotSpan`'s existing parse and display, so the two
representations cannot drift.

`quality` is stored as its four scalars rather than as a struct.
**`verdict` is read back on export, and that is correct — an earlier draft of
this paragraph forbade it on a misreading of `core/sidecar.rs:318` that this
one corrects rather than hides.** `Quality` derives `Serialize`, so
`Sidecar::to_json` writes whatever the struct holds; a projection that
recomputed `verdict` instead of restoring the stored value would hand a
reader a *different* document from the one imported, which is exactly the
failure §8's byte-for-byte corpus test exists to catch. `Sidecar::quality_of`
is a different function entirely — a query accessor that recomputes a
verdict from the runs *on request*, for `route.rs`'s lookup and `taiyaku
lookup`/`topology`'s printed verdicts — and nothing in the projection calls
it. So the column is never an authority in the sense that matters: a hand
correction to the runs makes `quality_of` answer something new immediately,
with no stored column to go stale beside it, while the stored bytes
themselves round-trip faithfully because storing them and answering
questions about them are two different jobs that happen to share a name.
`taiyaku-library/src/project.rs`'s `alignments_for` states the same rule at
the call site: "recomputing here would hand a reader a different document
from the one stored."

### 3.3 People, grouping, and external identity

```sql
person (id, name UNIQUE)
credit (id, person_id, role, series_id NULL, work_id NULL, edition_id NULL)
    -- role: 'author' | 'artist' | 'translator' | 'group' | 'publisher'
    -- A surrogate `id` rather than a composite primary key over the three
    -- nullable columns: SQLite permits NULLs in a non-INTEGER primary key,
    -- so a composite one would silently allow duplicate credits.

collection      (id, name, kind, created)       -- 'collection' | 'reading_list'
collection_item (collection_id, ordinal, series_id NULL, work_id NULL,
                 PK(collection_id, ordinal))

tag (id, name UNIQUE)  +  series_tag (series_id, tag_id)
                       +  work_tag  (work_id, tag_id)

external_id (series_id, provider, external_id,  PK(series_id, provider))
    -- provider: 'anilist' | 'mal' | 'mangaupdates' | …
```

**`credit.edition_id` is the one that earns its place.** "This volume was
translated by Cirno9baka and that one by Kodansha" is an edition-level fact.
`credits.rs:347` already extracts the group from `ComicInfo.xml`'s
`<ScanInformation>`, and there is nowhere in Komga or Stump to put it.

**`external_id` is in migration 1 despite nothing reading it yet**, on the rule
stated in §10: it holds **hand-matched** data. Deferring the table means
matching a whole library against AniList a second time. `user_credential` and
`sync_state` are *not* here, because they hold machine-obtained data with
nothing to backfill and nothing referencing them.

### 3.4 Per-person state

```sql
user (id, name UNIQUE, password_hash NULL, role, created)

progress (user_id, work_id, edition, slot, updated,
          completed_at NULL, read_count NOT NULL DEFAULT 0,
          PK(user_id, work_id)) WITHOUT ROWID

note (id TEXT PK, user_id, work_id, edition, slot,
      rx, ry, rw, rh, color, body, created, updated)

truth_page   (work_id, edition, page, PK(work_id, edition, page)) WITHOUT ROWID
truth_box    (id, work_id, edition, page, x, y, w, h, axis, kind, ignore, text)
truth_member (work_id, edition, ordinal, sha256,
              PK(work_id, edition, ordinal)) WITHOUT ROWID
```

**Two of those three tables exist because a document carries a fact a
projection of the present cannot reconstruct.** `truth_page` is a row per
page *looked at*, whether or not it held a box, so "examined and found
empty" survives as something other than "never opened" (Task 9).
`truth_member` is the digests the boxes were drawn **against** —
`TruthEdition::members`, which `is_stale_for` compares element-wise against
the archives in hand now, and which `taiyaku score` and `score_corpus` both
refuse on. Re-deriving it from the `member` rows that exist at read time,
as an earlier version did, made that comparison compare a list against
itself: it could never report stale again, and an export after a reimport
wrote a `taiyaku.truth.json` asserting archives the labelling had never seen
— over the reader's only copy of the true list (whole-branch review 2, C2).

**Read status is derived, except for the one case position cannot answer.**
Unread is the absence of a row, reading is a row, and read is `slot` at the
edition's last slot — none of which needs storing. `completed_at` and
`read_count` exist only for *finished once and now re-reading*, which a position
cannot express and which is exactly the state a "keep reading" shelf gets wrong.
They are in migration 1 by §10's rule rather than by preference: reading history
is behavioural data with **nothing to backfill**, so a later `ALTER TABLE` would
add the columns and leave them empty forever.

`note.id` is the ULID the reader already mints (`ulid.ts`, `serve/notes.rs:45`), kept
as the primary key so §4's per-note merge has an identity to merge on.

A note's region is four nullable columns rather than a JSON blob, all four NULL
together for a whole-slot anchor. `Region` is exactly `{x, y, w, h}` of `f64`
(`serve/notes.rs:28`) and is never extended by the reader without a format change, so
there is nothing a blob would buy.

`user` carries `password_hash` and `role` from migration 1 even though spec A
sets neither. Migration 1 inserts **one implicit local user** with a NULL hash,
and the server behaves exactly as it does today: no login, no hashing, no
guards. Spec A2 (§10) fills the column in and promotes that row into a real
account. `password_hash` is nullable for exactly that window and **A2 must make
it non-null**, because a nullable password on an internet-facing server is one
missed branch away from an account that authenticates with nothing.

### 3.5 Roots and search

```sql
root (id, path UNIQUE, added, scan_interval_secs, last_scan_at, last_scan_status,
      exclude_globs, edition_key, default_kind, default_language)
    -- edition_key/default_kind/default_language are what this root's material
    -- becomes by default; spec B's rules layer over them. No `auto_align`:
    -- see that spec's §9 for why it was removed rather than left unread.
meta (key PK, value)                   -- 'schema_version'
migrated_document (work_id, document, migrated, PK(work_id, document))
    -- document: 'progress' | 'notes' | 'truth' | 'library'
work_fts, series_fts                   -- FTS5, external-content over the titles
```

`migrated_document` is bookkeeping for §4.3's one-way migration and belongs to
no work's content: one row per work per sibling document, saying that document
has been settled and the file beside the sidecar is not to be read again. See
§4.3 for why that is a stored fact rather than an inference from emptiness.

`root` is fully defined here and written by spec B. Its config columns are one
line each: three for scanning (§10), `exclude_globs`, and the three that say
what this root's material becomes by default.
The scanner's *derived* state — per-directory mtime and entry count, for skipping
unchanged subtrees — is deliberately **not** here: it has nothing to backfill and
belongs in the spec that designs the walk against a real filesystem.

**Which users can see which root is not a `root` column.** It is access control,
it is per-user, and it belongs with roles in A2 rather than being half-built
here as a boolean that turns out to be wrong once roles exist.

FTS5 indexes `series_title.title`, so 「五等分の花嫁」, "Go-Toubun no Hanayome"
and "The Quintessential Quintuplets" all find the same shelf. It is an
external-content table over `series_title` and `work`, kept current by triggers,
so there is no second copy of the strings to drift.

## 4. Identity and the round trip

### 4.1 Identity is content, path is a hint

`work.content_key` is a sha256 over **the hub edition's** members' `sha256`s, in
member order — that edition alone, not every member of every edition. It is free
for an aligned work, since the sidecar already carries every member hash, and
`signature.rs` already hashes candidates for spec B.

**The hub is fixed when the work is created and the distinction is
load-bearing.** An earlier draft of this paragraph said "the work's members'
`sha256`s, taken in hub edition order", which reads two ways, and the other
reading is broken: if every member of every edition is folded in, then **adding
a second edition changes the work's identity** — and adding editions is the
entire point of this project. A rescan would then see the work vanish and a new
one appear, orphaning its progress and notes. Found while designing spec B
(`2026-08-14-library-scanner-design.md`, §7), which is the first thing that
would have hit it.

Matching an incoming sidecar to a row:

1. `content_key` hit → that work.
2. Miss, `sidecar_path` hit → **the same work with replaced archives.** Update
   the key rather than insert a second row.
3. Both miss → insert.

**Rule 2 depends on the hub's new content actually reaching the write, and
for a while it did not.** `import::sidecar` has always implemented it exactly
as written above, but before `bfc093b` (C1) the one caller anything ever
reached — `import::reimport`, which is what both `taiyaku library import` and
`taiyaku serve <sidecar>...` call — matched a work by path this way and then
wrote back a document built from *the database's own projection* of the work
it already held, never the incoming one. A re-solved hub's new archive hashes
therefore never reached the write, `content_key` never moved, and rule 2 was
reachable in `sidecar`'s own logic but not exercised by anything that called
it. C1 fixed this as a side effect of a larger correction (§4.2): the
incoming document's `work` block, hub name and hub edition now carry forward
into the write unconditionally, so the new archives' hashes are what
`content_key` is computed from and the row's key genuinely updates.
`import::tests::a_changed_hub_member_sha_updates_the_content_key_rather_than_leaving_the_old_one`
is the regression this rule needed and did not have: same path, a hub whose
member `sha256` changed underneath it, one work before and after, and the
stored key equal to the incoming document's own — not the one it replaced.

Move the sidecar, rename the directory, or remount the NAS at a different point
and progress, notes and collections all survive. The alternative — keying on the
canonical path, which is what `work_id` does today — was rejected because a NAS
mounting at `/mnt/nas` on one host and `/Volumes/nas` on another would duplicate
every work in the library and orphan every note on it.

### 4.2 Import reuses the merge rule rather than inventing a second one

The conflict this design was written to survive — *you re-solve a work with
`align --write` while the database holds hand corrections you have not exported*
— is already solved in this repo, and solved with more care than a fresh rule
would get. `Sidecar::merge_spoke` returns a typed `Merged` (`core/sidecar.rs:418`):

- **`CarriedHandRuns(n)`** — the spoke's bytes were unchanged, so corrections on
  the old edge were carried onto the new one.
- **`DroppedCredits(n)`** — the spoke's bytes changed, so a `credits` list was
  discarded rather than carried onto pages that may not exist. Dropped rather
  than refused, and the doc comment gives the asymmetry: a hand correction is a
  human decision that re-running nothing recovers, while a credits list is one
  command away.
- **`DroppedHandRuns(n)`** — added by `bfc093b` (C1), below.

So import projects through `Sidecar` and never merges rows:

```
1. project this work's rows           ──▶  an in-memory Sidecar
2. Sidecar::merge_spoke(incoming)     ──▶  the existing, tested rule, per spoke
3. write the result back to rows      ──▶  one transaction
```

**`taiyaku-library` therefore holds no alignment semantics whatever, with one
exception C1 added and this paragraph does not smooth over.** `merge_spoke`
still owns every rule about what a re-solve does to a hand correction — that
stays in `taiyaku-core`, tested there, with no second implementation to
disagree with it. But `merge_spoke` can *refuse*, overwhelmingly because the
hub moved underneath the spoke being merged, and `serve` must never fail to
start over a stale alignment (C1b): when it refuses, `import.rs`'s
`drop_stale_spoke` drops the stale stored edge and takes the incoming,
freshly-solved one instead, reporting what was lost as `DroppedHandRuns(n)`.
That is not a competing opinion about what merging means — it is what runs
*after* the tested rule has already declined to have one — but it is a
decision `taiyaku-library` makes rather than `taiyaku-core`, so "holds no
alignment semantics whatever" is now true of `merge_spoke`'s domain and not
quite of the crate as a whole. The same fix carries the incoming document's
`work` block, hub name and hub edition into the write unconditionally, on the
rule **a re-solve wins**: refusing to start over a stale alignment, which is
what this crate did before C1, was the defect (C1a, C1b), not a safety
property.

The `Merged` value is not swallowed: an import reports per work what the merge
did, in the same fixed-field style `--stats` and `duplicate-survey.sh`
established, so a person re-importing a library can see that three works carried
hand runs, one dropped a credits list and one dropped a hand-corrected run
because the spoke it was drawn against no longer exists.

### 4.3 The four siblings, which `merge_spoke` says nothing about

| Document | Rule as designed | Why that rule |
| --- | --- | --- |
| `<stem>.progress.json` | later `updated` wins | one row, no structure to conflict |
| `<stem>.notes.json` | union by ULID; per note, later `updated` wins | a note already carries an id and an `updated` (`serve/notes.rs:45`) |
| `<stem>.truth.json` | whole document, later wins | `TruthRegion` has no identity to merge on (`core/truth.rs:105`), and labelling is single-writer in practice |
| `<stem>.library.json` | field by field, later `updated` wins | **new, defined below**: the metadata the sidecar deliberately does not carry |

The truth rule is the weakest of the three and is recorded as such. It is
acceptable because a truth document is produced by one person at
`/label.html` in one sitting, and because losing one is a re-labelling rather
than a loss of reading. If two people ever label one work, this rule is wrong
and will need per-box identity.

**None of the four rows above shipped as written, on either side of the round
trip, and both departures are recorded rather than smoothed over.**

**Export never merges — it overwrites.** `export::work` (`export.rs`)
projects the database's own rows into all five documents and writes each one
through `atomic::write`, unconditionally. It never reads whatever file
already sits at the target path first, so there is no `updated` to compare
and nothing to union: the row this call started from is simply asserted onto
disk. That is consistent with §4.5's own case against write-through — export
is a caller's explicit decision, not a background sync — but it means the
table above never described export's behaviour, because export makes no
comparison for a rule to govern.

**Import gained a rule in C1's wake — `migrate.rs` (C3, `df4cb48`) — and it is
coarser than the table above, not the same rule moved to the other side.** On
a work's *first* import, for each of the four documents independently: if
the database already holds any state of that document's own kind for this
work — a `progress` row, a `note` row, a labelled `truth_page`, any
hand-fixed `librarydoc` field — the sibling file is left unread, even if it
exists and parses cleanly. If the database holds nothing of that kind yet,
the whole file is read and applied wholesale. There is no per-field or
per-note comparison of `updated` timestamps anywhere in this path: **the
database's holding *anything* of a document's kind wins outright, and a
sibling file only ever wins by being first.** This is a one-way migration for
an existing install's first import, not a live merge — nothing in
`migrate.rs` writes a sibling file. A malformed sibling is reported and
skipped rather than failing the whole import. §5's closing paragraph is the
fuller account of why this exists and what it reads; this section is the rule.

**"First" is a stored fact, not an inference from emptiness.** A row in
`migrated_document (work_id, document)` is written the first time each of the
four documents is considered — applied, left unread because rows already
existed, or not found on disk at all — and a document with such a row is
never revisited: the file is left unread whether or not it exists, and
whether or not the rows it once wrote are still there. Inferring it instead,
as the first version did, answered a different question. Delete your last
note and "the database holds no note for this work" becomes true again, so
the stale file was applied and the note came back — at every `serve` start,
forever, and nothing a reader could type made the deletion stick
(whole-branch review 2, C3).

`export --in-place` *now* removes a sibling document whose state has become
`None` (the same review's Minor 2), and that is the other half rather than a
substitute for this one: it runs when a person asks for an export, while the
resurrection above happened on every start, before anyone could have asked
for anything. The stored fact is what makes the deletion survive; the
removal is what makes the exported directory a faithful snapshot of it.
**"Migrated once" and "currently empty" must be distinguishable.** The one
outcome that writes no row is a malformed file: that document has not been
migrated, and fixing the file and importing again is how a reader recovers.
Losing the database is what makes the sibling files authoritative again, and
it takes these rows with them.

The finer rule never shipped for the reason the design conceded up front: a
note already carries the identity (`ulid.ts`) and timestamp a field-level
merge needs, but `progress` and `library.json` do not, and building
comparison machinery for four different shapes to serve one transitional
moment — an existing install's first import — was not worth it against "the
database wins, unless it has nothing yet."

### 4.4 `<stem>.library.json`, the metadata sibling

Hand-fixed metadata — a series grouping, a volume number, a chapter number, a
title in a second script — round-trips to a file for the same reason everything
else here does: so that losing the database costs a rescan rather than costing
the organizational work, which is the part nothing can re-derive.

```json
{ "taiyaku_library": 1,
  "updated":  "2026-08-14T11:03:22Z",
  "series":   { "titles": [{"title": "五等分の花嫁", "kind": "native", "primary": true},
                           {"title": "Go-Toubun no Hanayome", "kind": "romaji"}],
                "reading": "rtl" },
  "volume":   1,
  "editions": { "raw-mono": { "language": "ja", "kind": "raw",
                              "credits": [{"role": "artist", "name": "Negi Haruba"}] } },
  "members":  [{ "sha256": "3f9a…", "chapter": 1.0, "chapter_source": "hand" }],
  "tags":     ["romance"],
  "external": { "anilist": "97852" } }
```

**Members are keyed by `sha256`, never by path.** That is not a new convention:
`<stem>.truth.json` already keys an edition's members that way
(`core/truth.rs:134`, `RegionMember`), and it is what lets a chapter number
survive a file being moved or renamed — the same property §4.1 buys for the work
as a whole.

Collections are **not** in this file. A collection spans works and has no single
sidecar to sit beside; it lives in the database and is covered by §13's backup
rather than by export.

### 4.5 Export

`taiyaku library export <work>|--all [--dir DIR|--in-place] [--db PATH]`
writes all five documents, atomically, to `--dir` if given, or in place —
over the directory `sidecar_path` names — if `--in-place` is given instead.
**Under `--dir` each work gets a subdirectory of its own, named for its
`slug`**: `DIR/<slug>/taiyaku.json` and its four siblings, for one work or
for `--all` alike. That is not cosmetic. Every work in a real library keeps
its sidecar at `<dir>/taiyaku.json` — the convention that makes a work's
identity its `unique_slug` rather than its filename — and `export::work`
takes the filename *stem* from each work's own `sidecar_path`, so a flat
`DIR` gave every work the same five paths and each silently overwrote the
one before it: three works in, three files out, holding only the third
(whole-branch review 2, C1). `slug` is `UNIQUE` in the schema, so no two
works can share a subdirectory, and the five documents of one work stay
siblings of each other — which is what `migrate.rs` needs to find them by
stem on the way back in.
**Neither is a default the command will pick for you: given neither, it
refuses.** That refusal is C2 (`8b78603`), added after this section first
shipped, once a hand correction through `PATCH /api/works/{work}/alignments`
gave the sidecar a way to hold a change the database does not (§5's closing
note, and O43 in `docs/status.md`'s open items): unflagged, "export" would
have meant "write back over the very file that might be ahead of what is
being exported," silently, which is a worse default than asking the caller
to say which directory they mean. The database is the working store; **the
sidecar remains the interchange format**, which is what keeps "the sidecar is
the product" true rather than nominal.

Export is explicit rather than write-through. The alternative — debounce every
database write out to its sibling file, the way notes already save themselves —
was rejected because it reintroduces two writers for one document, which is
precisely the condition §4.2's merge rule exists to arbitrate, and it would run
that arbitration on every keystroke rather than once per import.

## 5. What changes in `taiyaku-serve`, and what does not

**`Work` survives**, and that is what keeps this spec small:

```rust
pub struct Work { id, path, sidecar: Arc<Sidecar> }   // unchanged
```

`sidecar` is now *projected from rows* rather than parsed from a file.
Everything downstream is untouched: `cover.rs`, `composite.rs`, `stump/`,
`opds.rs`, page serving, `?w=` resizing, the thumbnail cache.

| Thing | From | To |
| --- | --- | --- |
| `ServeState::works` | `load_works(&paths)`, parsing each file directly | still built from `&paths` — but each is imported into the library first (`library::import_canonical`), and the `Work`'s sidecar is `taiyaku_library::project::sidecar`'s projection of the row, not a fresh file parse |
| Work id | slugified filename, so the corpus collides (§1) | `work.slug`, so it does not |
| `progress` / `notes` / `truth` | sibling JSON | rows — **same routes, same `ETag` discipline** |
| `GET /api/works` | `["taiyaku"]` | objects: slug, title, series, volume, editions, cover |

**`Library::works()` does not exist, and the row above no longer claims it
does.** What ships instead is narrower than this section originally
described: `taiyaku serve <sidecar>...` still only serves the paths it was
handed, imported and re-projected, not every work the database happens to
hold.

`taiyaku_serve::works::from_library` was written as the shape this table meant
— a `Vec<Work>` from every row a `Library` holds — and **has since been
deleted** (whole-branch review 2, Minor 4). It never acquired a caller outside
`test_support.rs`, because nothing yet needs "serve whatever the database has"
over "serve what I was told to", and what `taiyaku serve` does instead is
`taiyaku-cli`'s `serve_work`: one path at a time, each `Work` built from the
row that path just imported to, with the path taken from the command line
rather than from `work.sidecar_path`. Two projections of one thing, one of
which only tests ran. Spec C1's browse API is the caller that would want it
back, and it is six lines — `SELECT id, slug, sidecar_path` and
`project::sidecar` — better written against C1's own answer to a work whose
`sidecar_path` is NULL than kept warm against a guess at it.

The `ETag` invariant survives intact. `serve/truth.rs:3` states it — note there
are two `truth.rs` in this workspace and this is the server's, not the core's:
*"one
serialisation shared by the file, the response body and the `ETag` over that
body, so a validator can never be computed over bytes a client was not sent"*.
It becomes one serialisation shared by the **projection**, the body and the
validator — the same rule with one fewer writer.

**`taiyaku serve <sidecar>...` keeps working.** It imports those paths —
through the same `import_canonical` that `taiyaku library import` calls — and
serves them from the rows. That is the migration path for an existing
install, and it is what keeps the three corpus-gated tests in
`serve/src/corpus.rs` alive without rewriting them.

**A path that cannot be imported is named on stderr and skipped** (added after
this section first shipped: review 2, I3). It used to propagate out of the
import loop, so one broken `taiyaku.json` — a renamed edition its `hub` no
longer names, a truncated file — took the whole server down along with every
other sidecar on the command line. The start refuses only when *nothing* could
be imported, since there is then no library to serve.

**The migration reads further than the sidecar itself.** `migrate.rs`
(C3, `df4cb48`, added after this section first shipped) is called once per
import, right after the sidecar's own rows are written: for each of
`<stem>.progress.json`, `.notes.json`, `.truth.json` and `.library.json`,
independently, if the database does not yet hold *any* state of that
document's own kind for this work, the sibling file — if one sits beside the
sidecar — is read and applied. If the database already holds state of that
kind, the file is left unread even though it still sits on disk: **rows are
current, files are history**, and this is a one-way migration for an
install's first import, not a live sync — nothing here ever writes a sibling
file, and a document already migrated is never revisited on a later import.
A malformed sibling is reported and skipped rather than failing the whole
import. §4.3 has the rule in full, including why it is coarser than what that
section originally designed.

**Web-side, one change and no new screens.** `openFirstWork` still opens the
first work, from a richer payload; `api.ts`'s `works()` return type changes. The
home screen is spec C1. Keeping it out of here is deliberate: a browse UI built
before spec A2's guard exists would be built twice.

## 6. Crate layout and dependencies

```
taiyaku-core ──┬──▶ taiyaku-read ──┬──▶ taiyaku-serve ──▶ taiyaku-cli
   (wasm)      │                   │           ▲
               └──▶ taiyaku-wasm   └───────────┤
                                   taiyaku-library
```

`taiyaku-library` is depended on by `taiyaku-serve` alone. It **must not** be
reachable from `taiyaku-core` or `taiyaku-wasm`, which compile to WebAssembly.
`taiyaku-cli` picks it up transitively, which is where the
`taiyaku library import|export` subcommands live.

```
crates/taiyaku-library/src/
  lib.rs        Library: open, migrate, transactions
  schema.rs     migrations, versioned in `meta`
  identity.rs   content_key
  project.rs    rows  ──▶ Sidecar / NotesDoc / TruthDoc / Progress
  import.rs     documents ──▶ rows, through Sidecar::merge
  export.rs     rows ──▶ documents, atomically
  work.rs       browse queries
  user.rs       the user table; the mechanism is spec A2
```

**`rusqlite`, over three alternatives, for reasons specific to this repo.**

- **`sqlx`** buys compile-time query checking, and that is expensive here: it
  needs a live `DATABASE_URL` at build time or a committed `.sqlx` offline cache
  regenerated on every schema change, against a CI with **no cargo cache**,
  `--all-features`, and `timeout-minutes: 10` — where a stale cache fails the
  build in a way that reads as unrelated. Its other advantage, async, is
  illusory for SQLite: sqlx runs it on a background thread pool, and this
  codebase already has the right pattern for blocking work under axum in
  `stump/`'s resolution and `cover.rs`'s decode, both inside `spawn_blocking`.
- **`diesel` / `sea-orm`** put a second opinion about the model into the
  dependency, which §4.2 exists to prevent: the store is a dumb projection and
  every semantic lives in `taiyaku-core`.
- **`libsql` / `turso`** buy nothing a single-process server needs, at the cost
  of a maturing fork.

The honest cost of rusqlite is no compile-time query checking, so a typo'd
column name is a runtime error rather than a build one. §8's byte-for-byte round
trip is the mitigation: it exercises every column of the projection, which is
where such a typo would live. Pooling is manual, which at this write volume —
progress and notes — means one writer connection rather than a pool.

Komga and Stump both run on SQLite themselves, which is weak evidence and is
offered as such: it establishes the shape is workable for this class of app and
nothing about the crate choice.

**The `bundled` question is deferred to the plan with a measurement attached.**
`bundled` compiles SQLite's C source and removes a
system `libsqlite3-dev` from the CI image; it also compiles on **every** CI run,
because `.forgejo/workflows/ci.yml` has no cargo cache and both the clippy and
test jobs carry `timeout-minutes: 10`. `docs/status.md` records `tract-onnx`
already costing 21 s of that budget on 32 cores with a warm registry, and more
on a small container. The plan measures `bundled` against the system library on
both jobs and picks; **no figure is asserted here, because none has been taken.**
FTS5 is needed either way and is a feature of both paths.

`argon2` is not a dependency of this spec. It arrives with A2.

## 7. One refactor this needs

`atomic::write` lives in `taiyaku-serve` and `taiyaku-cli` already reaches
across the workspace for it (`regions.rs:249`, `main.rs:1027`). Export needs it
and `taiyaku-library` cannot depend on the server. It moves down to
`taiyaku-read` and the three call sites are updated in the same commit. **No
re-export and no deprecation window**: the workspace is closed, this crate has
no external consumers, and `cargo` names every call site that has not been
updated. An earlier draft of this section proposed a `taiyaku_serve::atomic`
re-export "for one release" — the project has no releases, and the phrase was
importing a convention from a world that does not apply here.

This is the only pre-existing code this spec moves. Nothing else is refactored.

## 8. Testing

**Units** run against `:memory:` databases — no fixtures on disk, no cleanup, no
`TAIYAKU_CORPUS`.

**The headline corpus test**, gated on `TAIYAKU_CORPUS` alongside the standing
checks already there: *every corpus sidecar survives sidecar → import → export
byte-for-byte.* `Sidecar::to_json` is deterministic — `BTreeMap` keys,
pretty-printed, and `sidecar.md` pins the float formatting down to `avg_run`'s
five decimal places — so **byte-identical is the honest bar** rather than
semantic equality, and a projection that loses a field fails loudly instead of
passing a comparison written to tolerate it.

That test is the reason §3 stores four slot columns rather than the rendered
span string, and the reason §3.2 forbids reading `verdict` back on export: both
are places where a lossy projection would otherwise pass every unit test.

**The merge cases** are unit-testable without the corpus and must be tested at
the store boundary as well as in `taiyaku-core`: a re-import that carries hand
runs, one that drops credits, and one where a hand-corrected span no longer
exists in the re-solved alignment.

**Existing tests.** `taiyaku-serve`'s suite drives `build_router` with
`oneshot`; `test_support::work()` gains a `:memory:` library. The three
corpus-gated tests in `serve/src/corpus.rs` are covered by §5's "`serve
<sidecar>...` keeps working" and should need no change — **which is a claim to
verify early in the plan rather than at the end**, because it is load-bearing
for the branch shape.

## 9. What is deliberately not done

- **No scanner.** No directory is walked. Works enter the library through
  `taiyaku library import` or through `taiyaku serve <sidecar>...`. Spec B.
- **No authentication.** The tables exist, one implicit user is inserted, no
  route is guarded. Spec A2.
- **No browse UI, no home screen, no search UI.** FTS5 tables exist and nothing
  queries them. Spec C1.
- **No AniList sync.** `external_id` exists and nothing writes it. Spec G.
- **No download route**, so the OPDS deviation of `serve/opds.rs:162` stands for
  now and offline reading still does not work. Spec C2, designed in §13.1.
- **No metadata editing UI, and no ComicInfo write-back.**
  `<stem>.library.json` is defined (§4.4) and exported; nothing offers a screen
  to edit it in. Spec C3, with the write-back's gates designed in §13.2.
- **No import of a Komga or Stump library's structure, ever** — not deferred,
  declined (§13.6). The scan is the ground truth. Reading progress alone
  migrates, as a one-off command, spec J.
- **No backup schedule.** §13.4's `VACUUM INTO` command is designed and not
  built here, which is a stated risk rather than an oversight: between this spec
  and that one, the database is the only copy of nothing yet, because nothing
  has written a collection or a read count into it.
- **No EPUB or PDF reading.** `member.format` records what a scanner *detected*,
  including formats nothing can open yet — because the alternative is a scanner
  that silently drops them, and *"a candidate that reached the page threshold
  and then vanished from the output is the silence this whole project is written
  against"* (`docs/status.md`, of `inventory`).
- **No reflowable-ebook position model.** `progress` keys on `(edition, slot)`.
  A spine index and a CFI is `ALTER TABLE progress ADD COLUMN locator TEXT`,
  which SQLite does without rewriting the table, so building it now is cost with
  no payoff.
- **No multi-user anything beyond the column.** One implicit user, and every
  per-person row already carries its id.

## 10. Where the rest goes, and the rule that put it there

**The rule:** migration 1 holds what spec A writes, **plus anything hand-entered
or expensive to re-derive**. Everything else is a later `CREATE TABLE` or
`ALTER TABLE`, both of which SQLite does cheaply and additively.

By that rule `external_id` and `member.chapter_declared` are in migration 1 —
the first is hand-matched, the second is free while a scanner has the archive
open and costs a full re-read of every member to backfill. And `user_credential`,
`sync_state` and the scanner's per-directory state are not — all three hold
machine-obtained data with nothing to backfill.

| Spec | Delivers | Depends on |
| --- | --- | --- |
| **A** *(this one; shipped 2026-08-18, #76)* | the store, import/export, `serve` off the database | — |
| **A2** *(shipped 2026-08-18)* | argon2id, sessions, **API keys / HTTP Basic for OPDS**, guards, roles, per-root visibility, `FILE_DOWNLOAD`-equivalent — designed in `2026-08-14-library-auth-design.md`. Per-root visibility ships as a resolved `Principal.roots` that no route filters on yet, and the capability flags likewise: B landed first and gave `work` no `root_id`, so C1's browse API is the first caller that can act on either (O54) | A |
| **B** *(shipped 2026-08-18, #77)* | the scanner: facts on disk, a user-editable rule layer over them, proposals, change detection — **standing alone, per §13.6**; designed in `2026-08-14-library-scanner-design.md` | A |
| **C1** | browse API and UI: shell, series page, search, and the reader gaining an entry point — designed in `2026-08-14-library-browse-design.md` | A2, B |
| **C2** | download: member bytes, and an edition as one streamed CBZ (§13.1) | A2 |
| **C3** | curation: the proposals inbox and metadata editing — designed in `2026-08-14-library-curation-design.md` | C1 |
| **C4** | configuration: roots, scan control, spec B's rule editor and its per-field provenance, A2's user management, backup — designed in `2026-08-14-library-configuration-design.md` | C1 |
| **D–F** | project 2: extract the alignment pipeline, a job runner, align from the UI | A, C |
| **G** | AniList / MAL sync: `user_credential`, `sync_state`, the push loop | A, and a chapter number worth pushing |
| **J** | `taiyaku library import-progress`, one-off, off Komga and Stump (§13.6) | A, B |
| **H** | fixed-layout EPUB as a member format | B |
| **project 3** | reflowable EPUB: a spine-and-CFI position model and a second reader | H |

Three of those rows exist because this spec's own prose placed them nowhere.
**G** waits on more than spec A: pushing chapter progress means having chapter
numbers, and `chapter_at` refuses to infer one from a filename
(`core/sidecar.rs:477`), so the useful version of G follows a scanner that has
read `ComicInfo.xml` and a person who has corrected what it found. **H** is a
container change and no schema change; **project 3** is a different reading unit
and a different reader, and `progress` gains a `locator` column when it arrives.

**The Stump page source becomes a migration path rather than a feature.** The
stated goal is replacing this Stump instance, so `composite.rs`'s Stump branch
exists to serve pages during the changeover and to let a member live only on
Stump until it does not. §5 lists it "unchanged", which is true of spec A and
ducks this; nothing removes it, and nothing should build on it either.

**A2 lands before C** so the browse UI is built behind the guard rather than
retrofitted into it. *(It did — but B landed before A2 rather than after, which
that spec's §8 had predicted the other way round. The consequence is recorded
there and in O54: nothing changed about the ordering rule, only about which spec
was available to populate the filter.)* **OPDS clients need HTTP Basic or an API key rather than a
session cookie** — Panels and Chunky cannot hold one — and that is the path that
matters most here, since the OPDS feed is what made this project's grouping
visible to a real reader in the first place.

**On spec B's change detection.** *This paragraph's original argument was
corrected on 2026-08-14 and is rewritten rather than kept, because unlike this
file's other retained-and-marked passages it was never true of the system being
built.* It reasoned that a `notify` watcher delivers nothing over NFS, that
`docs/status.md` calls the corpus *"held on a NAS"*, and that the interval scan
must therefore be short. The NFS observation is correct **of this workstation**,
which mounts the library over nfs4; the server will run **on** the NAS, where
the same tree is local. The conclusion was about the wrong host.

What replaces it is better than a corrected version of the same reasoning, and
is designed in that spec's §8: **whether a watcher works is probed by
experiment, not predicted from a filesystem type.** No fstype table can know
about an Unraid user share (FUSE `shfs`), a Docker bind mount or `overlayfs`, so
the scanner writes a temporary file into the root, watches for the event, and
records whether it arrived. That is correct for this deployment, for the NFS
case other people will have, and for filesystems nobody has considered. A
recursive watch costs one inotify watch per directory; this machine allows
524,288 watches and 1,024 instances (measured 2026-08-14), and a lower ceiling
elsewhere must be reported rather than silently degrading to a partial watch.

## 11. Branch shape

One branch, one squashed PR, per `README.md`'s contributing section. Ordered so
that each step is reviewable on its own and the risky claim is tested first:

1. Move `atomic::write` down to `taiyaku-read` (§7).
2. `taiyaku-library` crate, migration 1, `:memory:` units.
3. `identity.rs`, `project.rs` — and the byte-for-byte corpus round trip (§8),
   which is where a lossy projection surfaces.
4. `import.rs` through `Sidecar::merge`; `export.rs`.
5. `ServeState` off the database; `progress`/`notes`/`truth` onto rows; the
   `/api/works` payload.
6. `taiyaku library import|export` subcommands.
7. The measurement churn (§12).

## 12. The measurements this invalidates

A new crate moves every coverage and count figure in the repo. This is part of
the work, not a surprise at the end:

- `scripts/coverage-floor.mjs` — the Rust floor
- `scripts/status-test-count.mjs` — the test count
- `docs/status.md` — "Five crates, 764 tests", the dependency list, the
  crate table, the `serve` CLI paragraph, and §1's reversal of "deciding which
  files constitute a library is a different job"
- `README.md` — the `taiyaku serve` synopsis and "Using it"
- `docs/sidecar.md` — a note that a database is now a second consumer of the
  format, that the format remains the interchange, and that `<stem>.library.json`
  (§4.4) joins `<stem>.notes.json`, `.truth.json`, `.progress.json` and
  `taiyaku.stump.json` as a sibling convention

## 13. Komga and Stump parity: five things nobody had named

These came out of an audit *after* the design was settled, on the question "is
everything else scoped out". The answer was no. None of them reopens migration 1
beyond the four columns already added above (`progress.completed_at`,
`progress.read_count`, `root.exclude_globs`, and `root`'s three edition
defaults), which is why
they are recorded here rather than folded into §3.

### 13.1 Download — streamed, `STORE`, uncached

**There is no download route today**, and the OPDS acquisition link points at a
cover image rather than a file. That is deliberate and documented as such
(`serve/opds.rs:162`, *"a deliberate deviation, decided by the owner"*): OPDS 1.2
requires an acquisition link on every entry, and page streaming was intended to
be the real mechanism. The consequence, which the deviation did not state, is
that **offline reading in Panels does not work**.

Two things become downloadable: a **member**, as its original bytes — unchanged,
proxied from Stump where Stump holds it — and an **edition**, as one CBZ.

The obvious design for the second is to generate and cache it the way covers are
cached, and it is the wrong one, because **the pages are already JPEGs**. A CBZ
of compressed images uses `STORE`, not `DEFLATE`, and once it does:

- **It streams.** Generation is a copy, so there is no cache, no eviction, no
  first-request block on zipping 200 pages, and no disk cost for a ~400 MB
  volume. The `Condvar` single-flight `cover.rs` needs is not needed here.
- **`Content-Length` is exact and known before the first byte**, because a
  `STORE` zip's size is a deterministic function of its entry names and sizes,
  both of which the sidecar already records.
- **`Range` therefore works**, which is what an OPDS client actually wants for a
  large download over a connection that drops.

Entries are named zero-padded and sequential, in the edition's **page** order per
`sidecar.md`'s normative rule — so the download reads in the order the reader
shows it, and stored order matches name order rather than joining the *32 of 57
archives sort differently* trap that rule exists to document. A generated
`ComicInfo.xml` goes in from the library metadata.

Both paths need a per-user download permission, which is A2's.

### 13.2 Metadata editing — and the one feature that writes to your library

The sibling file of §4.4 is where an edit lands by default. **A second, opt-in
target writes the metadata into the archives' own `ComicInfo.xml`**, which is
what Komga optionally does and what the owner asked for as a UI option.

It is the first feature in this project that writes to the library, against a
stated design principle — *"Your library is not modified. Archives are read,
never rewritten"* (`README.md`). The objection is not philosophical. **Rewriting
a zip changes its `sha256`, and that hash is load-bearing in three places at
once:** every sidecar member records it as the staleness check, the cover and
thumbnail caches are content-addressed by it (`serve/cover.rs`), and §4.1's
`content_key` is derived from it. Stamping one archive therefore invalidates the
sidecar, regenerates every thumbnail of that member, and changes the work's
identity.

So it ships as an **aimed tool rather than a save path**, in the spirit of
`inventory` and `duplicates` printing and leaving the acting to a person — except
that here taiyaku acts, so the gates are the design:

- explicit, per-work, confirmed; never bulk and never automatic;
- the new `sha256` propagates into the sidecar, the member row and `content_key`
  in one transaction, or the write does not happen;
- refused for `rar` and `7z` members, since only zip is written;
- a zip cannot be edited in place, so it is a full copy and an atomic rename —
  which on this corpus's archival TIFF volume at 430 MB per page is not a cheap
  click, and the UI should say the size before it starts.

### 13.3 Read status

Settled in §3.4: derived, except `completed_at` and `read_count` for the
re-reading case. Nothing further.

### 13.4 Backup, which is now load-bearing

The database is the authority for notes, collections, tags, external ids, users
and read history. **Sidecar export is not a backup and the distinction matters**:
§4.5 writes five documents per work and carries none of the cross-work data, and
`<stem>.library.json` deliberately excludes collections because a collection has
no single sidecar to sit beside.

`taiyaku library backup [--out DIR]` uses SQLite's own `VACUUM INTO`, which takes
a consistent snapshot of a live database without stopping the server and needs no
external tool. Restore is stop, move the file, start. It runs on a schedule with
a retention count, because a backup nobody remembers to take is not one.

### 13.5 Per-root settings

`root.exclude_globs`, plus `edition_key`, `default_kind` and `default_language`,
added to §3.5. **No `auto_align`** — proposed here, then removed while designing
spec B, whose §9 records both the reason and what would have to be true for
automatic alignment to be reasonable. Which users see which root is A2's, per
§3.5's note.

### 13.6 Migrating off Komga and Stump: progress, and nothing else

**The scan is the ground truth, and spec B may not depend on an external source
to reach it.** Settled by the owner on 2026-08-14: importing a library's
structure from the Komga or Stump instance it is replacing would be a nice
feature and is explicitly not wanted, because a scanner that leans on one is a
scanner whose grouping was never tested. If B's series and volume grouping is
not good enough to stand alone, that is a defect in B rather than a case for an
importer, and hiding it behind an import is how it would stay hidden.

**The one thing worth carrying across is reading progress**, because it is the
only data in those instances that a rescan cannot reproduce. It ships as a
one-off command rather than a sync:

```
taiyaku library import-progress --from komga|stump --url URL --key KEY [--apply]
```

Printing by default and acting only under `--apply`, which is the discipline
`inventory` and `duplicates` already hold: *they print what they found and leave
the acting to a person*.

**Two problems it has to solve, neither of them the HTTP.**

**Matching.** Neither system's content hash is a sha256 of the file, so
`content_key` does not transfer and the match has to be made another way:
absolute file path first — both expose one and the same host serves both — then
basename plus page count, then nothing. Every unmatched book is **printed**, with
what was known about it. A migration that silently carries 80% of a library is
worse than one that carries 80% and says which 20% it dropped.

**Units.** Komga and Stump track a page within a *book*; taiyaku tracks a slot
within a *work*. A book is one **member**, so the conversion is: page within the
member → flat page index in the edition, by adding the member's ordinal offset →
slot, through `slot.rs`'s spread expansion, since a joined spread is two slots
and one page. Every piece of that exists; none of it is a lookup.

Completion maps onto §3.4's two columns, which is a second reason they are in
migration 1 rather than derived: a Komga book marked read at page 1 of a
re-read is a state `slot` alone cannot represent, and it would arrive on the
first import.

Conflict rule: skip a work that already has progress, name it, and require
`--force` to overwrite. This is a migration, run once, against a database that
should be empty of progress at the time.

**Both APIs' exact shapes are unverified.** Komga's published REST reference
404s at the URL tried on 2026-08-14, and a search returned nothing at field
level for Stump. This does not need resolving from documentation: **both
instances are running and reachable**, so the plan's first step is one
authenticated request each, recording the actual book and read-progress objects
into `notes.md`. That is stronger evidence than a document, and it is the house
standard — measured on this machine rather than recalled.

**Scoring the scanner against Komga is not importing from it.** A use for that
instance that keeps the scan as ground truth: after B ships, score its grouping
against Komga's — how many candidate volumes landed in the right series with the
right number — the way `inventory` was measured on the One Piece collection at
579 directories down to 141 volume directories forming 120 volume-editions. That
is a measurement, not a dependency, and it is how B finds out whether it is
actually good enough to stand alone.

## 14. What could not be measured

- **`bundled` versus system SQLite on CI.** No local Docker to run the forge's
  image (`docs/status.md` records the same limitation for `node:26-bookworm`),
  so the compile cost is measurable on this machine and only estimable on the
  runner. §6 defers the choice to the plan with the measurement attached rather
  than guessing here.
- **Komga's and Stump's API shapes.** Komga's published REST reference 404s at
  the URL tried on 2026-08-14 and a search returned nothing at field level for
  Stump, so §13.6 asserts no field names. Both instances are running and
  reachable; spec J's first step is one authenticated request each, recorded in
  `notes.md`.
- **Query performance at library scale.** Every figure available is from a
  twelve-work corpus. Nothing here should be quoted about a library of
  thousands until a scan has produced one, which is spec B.
- **Whether `serve/src/corpus.rs`'s three tests survive §5 unchanged.** Claimed
  from reading them, not from running them against a store that does not exist.
  §11 puts it early for that reason.

  **Measured on 2026-08-14. The claim holds, with one qualification worth
  stating rather than smoothing over.** All three pass with what they assert
  untouched — 12.7 s with `TAIYAKU_CORPUS` set. But
  `every_page_the_server_serves_is_the_page_the_aligner_sequenced` is the one of
  the three that constructs a `ServeState` by hand, so its source needed a single
  field added to that struct literal once `ServeState` gained one. The other two
  never construct a `ServeState` and compiled untouched.

  So the honest form is: **their logic survived; one of their source files did
  not compile untouched.** The distinction matters because "needs no change" was
  what the branch shape was resting on — and the branch shape was right.

# Duplicate resolution as a command — design

**Status: built, with six named deviations.** Settled in brainstorming on
2026-08-01 and shipped the same day. This builds §2 of
[`../../status.md`](../../status.md), "Duplicate resolution as a command", from
the protocol worked out by hand in [`../../notes.md`](../../notes.md) under
"Sorting 109 volumes' worth of duplicates" and "Choosing a base edition, and a
mislabelled file caught in the act".

Every page count quoted below was counted against `corpus/` on 2026-08-01. Every
runs-and-pairings figure is quoted from `notes.md`, which measured it. Where this
document expects a result it has not measured, it says so and names it as
something the first run establishes rather than something the design asserts.
**Two of the results it did assert came back the other way**, and the
corrections are made in place below as well as named here.

> ## Deviation: six claims that are not true of what shipped
>
> Both commands shipped on 2026-08-01, with the argument shapes, the
> signature-then-content-hash step, the all-pairs step and the ranking as
> designed, and the five gated corpus checks described below are the five that
> run. Six claims in the text were wrong, and are corrected in place rather than
> left standing: a design document nobody re-reads against the code is exactly
> how a wrong reading survives, which is what happened to the v092 claim here for
> a day after five other files had been corrected.
>
> **1. One Piece v092 is containment, not identity.** This document asserted
> "Identity, 200 of 200 slots, one run", in the spreads decision and again in
> corpus check 2. Measured: c0 is 200 slots and c1 is **202** — every one of
> c1's 101 images detects as a spread, so expansion doubles its count and leaves
> a half-slot unreached at either end. All 200 of c0's slots pair, so that is
> coverage on one side only, which is `Within` and not `Identity`; the two stay
> two clusters, c1 ranks first on its 202 slots, and `corpus.rs` asserts both.
> The root cause is recorded in `notes.md`: `align` reports unmatched slots for
> the **A side alone**, so a solve covering all of A prints "nothing unmatched"
> whether or not B carries slots nothing reaches, and containment reads as
> identity to anyone reading that output. This document inherited the reading
> and was the fifth file to carry it.
>
> **2. `Relation` has five variants, not three.** The design distinguishes
> identity from distinct and admits nothing between them. `Within` and
> `Contains` were added mid-branch on the measurement above, because containment
> is the commoner shape: of the five corpus pairs that come within two slots of
> covering each other, **three are containment** — v092's c0 within c1, Bocchi's
> `hamaji` containing `krc`, and Chainsaw Man's `src-a` within `src-b` — and
> only two are identity. Clustering still merges on identity alone, for the
> reason the design already gives: a container holds content the other copy
> lacks, and the ranking prefers it on slots anyway.
>
> **3. The `set:` line ships in a different field order, and `clusters` counts
> every cluster.** Shipped:
> `copies signature clusters flagged refused fragmented slots decided best`,
> with `best` **last** rather than before `slots`, because a corpus path can
> contain a space and no field after one would survive splitting. And `clusters`
> counts every cluster the collapse produced, flagged copies included, so a
> two-copy set with a refusal between them prints `clusters=2 flagged=2`: the
> field states what the clustering found, and what was kept out of the ranking
> is `flagged`'s job to say. The worked example below is corrected for the
> order. Its tables are still the designed sketch — the shipped rendering prints
> a `copies` table first, then pairs, then clusters, and names copies by index —
> and `duplicates.rs`'s `tables()` tests pin that form verbatim.
>
> **4. `duplicates` writes the fingerprint cache.** "Neither command moves,
> deletes or writes anything" is true of the library and false of the disk:
> every cluster representative is fingerprinted through `cached_edition` into
> `.taiyaku-cache`, shared with `align`, which is what makes all-pairs
> affordable at all. `status.md` states the property that is both true and worth
> stating — **neither of them moves, renames or deletes anything** — and this
> document now says that instead.
>
> **5. The candidate walk is two passes, not one.** "One pass, bottom-up" cannot
> work, and `inventory.rs`'s doc comment records why: deciding each directory's
> fate as the recursion unwound put three sub-threshold chapters into `excluded`
> moments before the volume containing them qualified — the exact case the
> candidate rule promises does not happen. `totals` now measures the whole tree
> with no side effects, and `decide` walks it again top-down, descending only
> where no child qualifies on its own.
>
> **6. The content confirmation hashes pages, not `member_digest`.** The design
> named `taiyaku_read::member_digest`; `signature::content_digest` shipped
> instead, and the difference is the whole point of the step.
> `member_digest` hashes an archive's *file* bytes and a directory's *page*
> bytes, so it reports two answers for one scan filed two ways — which is
> precisely the case a signature match is most likely to be reporting. Hashing
> pages makes the confirmation packaging-independent in the same way the
> signature is.
>
> The two results this document deliberately declined to predict both came back
> on the side of the stricter reading: Bocchi's `hamaji`/`krc` pair is
> containment rather than "the identity case again", and the Chainsaw Man
> identity cluster is `{src-a, src-c}`, the two 198-image scans. `notes.md`
> records both under "Duplicate resolution as a command, and half the identity
> claims above are containment".
>
> **Five sentences are corrected in place without notes of their own**, each
> having promised output that does not exist. A copy that refuses some of its
> pairs and pairs with the rest is *kept* in the ranking with its refusals
> visible in the pairs table; there is no separate "inconsistent" report and none
> was built. A candidate whose name yields no number lands in the `?` row of the
> table rather than in a section headed `no volume number`. A signature's page
> count and byte total are carried but never printed; only the digest is used. A
> signature match the content hash refutes falls through to alignment silently
> rather than being reported as such. And a tie at the top of the ranking prints
> as a tie and sets `decided=no`, but nothing in the output volunteers that
> `--against` would break it.
>
> Two smaller stalenesses go the same way: `signature.rs` holds the content hash
> that confirms a signature match rather than any formatting of a `Signature`,
> which does not exist, and `main.rs`'s line count is now given as what it was
> when this was written rather than as what it is.

## What §2 asks for, and what it leaves out

§2 states the protocol in three steps:

1. **Signature** — file count, total bytes, sorted size list. Settles exact
   duplicates without decoding anything.
2. **All-pairs alignment within the set** — not everything against the first
   copy, which hides clusters.
3. **Rank what remains** by content first (slots, then pairings), then by how
   cleanly it aligns.

It leaves out the step that came before all three. Getting from 579 directories
to "141 are volume directories, forming 120 volume-editions, 19 of which have
more than one copy" was shell work, and it is the part that depends on how a
particular collection is named rather than on what is inside the files.

**Both halves ship, as two commands.** The inventory half is where the guessing
lives and the resolution half is where the measurement lives, and keeping them
separate is what lets the first one be wrong visibly.

```
taiyaku inventory  <root>            [--min-pages N] [--pattern SPEC]
taiyaku duplicates <copy>... -- <copy>... [-- <copy>...] [--against <member>...] [--ltr]
```

Named `inventory` rather than `survey` for two reasons: it is the word
`notes.md` already uses ("Inventory first, and most of it is not volumes"), and
`scripts/margin-survey.sh` has taken `survey` for a different thing.

## `taiyaku inventory`

Walks a root, decides which directories hold a volume, groups them by a volume
number parsed from the name, and reports which groups have more than one copy —
with a signature per copy, so the groups that need no alignment and no transfer
are visible before anything is copied anywhere.

```
$ taiyaku inventory /nas/_op
walked 579 directories | 141 candidates | excluded 438 (15-28 pages)

vol  copies  signature   paths
001  2       distinct    .../v001, .../HQ/001
069  3       2 of 3      .../v069, .../HQ/069, .../v01-100/v069
077  3       distinct    .../v077, .../HQ/077, .../v01-100/v077
081  2       identical   .../v081, .../HQ/081
...

inventory: dirs=579 candidates=141 volumes=120 duplicated=19 settled=3 \
           excluded=438 excluded-pages=15-28
```

The table goes to stderr and the `inventory:` line to stdout, the same split
`duplicates` uses and for the same reason. Its fields, in fixed order:
`dirs` walked, `candidates` reaching the page threshold, distinct `volumes`
after grouping, `duplicated` volumes with more than one copy, `settled` those of
the duplicated ones whose copies all share one signature, `excluded` candidates
below the threshold and the `excluded-pages` range they spanned. `settled` is a
fraction of `duplicated` and is counted over the same groups, so a group of one
never counts by agreeing with itself.

### What counts as a volume directory

**A directory is a candidate when its recursive page count reaches
`--min-pages` and none of its children reach it on their own.** Two passes: one
that measures every directory's recursive total with no side effects, and one
that walks the same tree top-down and descends only where no child qualifies on
its own. Deciding as a single bottom-up recursion unwound was tried and cannot
work — it commits a chapter to `excluded` before it can know whether the volume
above it qualifies.

This is the rule rather than "a directory that directly holds images" because
`sidecar.md` already records that some editions nest — seven of the corpus's
fifty-seven archives put chapters in subdirectories — and a volume split into
seven chapter directories of twenty pages each holds no images directly. Under
this rule the volume directory is the candidate and the chapter directories are
not, which is the answer that matches how the edition reads.

**Its failure mode, named rather than hidden.** A directory holding nothing but
material below the threshold — a folder of 438 weekly Jump issues, say — has no
qualifying child, so the folder itself becomes one enormous candidate. That is
visible the instant it happens: the candidate's page count is printed, and 8,000
pages does not look like a volume. It is not silently correct and not silently
wrong; it is on the screen.

`--min-pages` defaults to **60**. Measured rather than picked from the air: the
material excluded in the One Piece run ran 15 to 28 pages, and the smallest real
volume in the corpus is One Piece v092's all-spread copy at 101 pages. Sixty
sits above the first and well below the second. **The excluded page range is
printed** — `excluded 438 (12-28 pages)` — so a threshold that nearly cut
something is visible. If the top of that range sits at 58 against a threshold of
60, the call was close and you can see that it was.

### How two directories come to claim the same volume

The volume number is parsed from the directory's own name. The default takes the
first digit run immediately preceded — optionally across one space — by `v`,
`vol`, `vol.` or `第`, case-insensitively, falling back to the first digit run in
the name when none of those appear.
Leading zeros are stripped, so `v007`, `v7` and `第7巻` group together.

**The parse is printed next to every path.** A grouping key derived from a name
is a guess, and a guess that is not shown is one nobody can correct.

`--pattern` overrides it with a literal containing exactly one `{}` placeholder:
`--pattern 'v{}'`, `--pattern '第{}巻'`. The placeholder matches a digit run and
everything else matches literally.

**Deliberately not a regex.** `status.md` states this workspace's dependency
list as a fact about the project — `image`, `zip`, `sha2`, `rayon`, `base64`,
`serde`, plus the server's four and the binding's one — and `regex` pulls in
three more crates to do a job a thirty-line literal matcher does. Every name in
`notes.md`'s inventory is covered by the default or by a one-placeholder
literal. If a collection turns up that neither handles, that is evidence for the
dependency and it can be added then, on the evidence rather than in advance.

A directory whose name yields no number is grouped under `?` in the `vol`
column and printed whether or not it has company there, rather than dropped,
because a candidate that reached the page threshold and then vanished from the
output is the kind of silence this repository exists to avoid.

**The root is assumed to hold one series.** Two series under one root with
overlapping volume numbers would group across them. That is a real limitation
and it is not addressed: the material that motivated this is one series'
release, and inventing a series key with nothing to measure it against would be
a guess stacked on a guess.

## `taiyaku duplicates`

Takes the copies of one volume, `--`-separated in the manner of `topology`, each
copy being one or more members. Runs the three steps and reports what is left to
decide.

```
$ taiyaku duplicates corpus/incoming/op-duplicates/v077-mono-c{0,1,2}
signature: 3 copies, 3 distinct

clusters
  {c1, c2}  identity   234 slots, 234 paired, nothing unmatched
  {c0}      distinct   236 slots

pairs
  c0-c1  104 runs  184 of 236  FRAGMENTED
  c0-c2  100 runs  186 of 236  FRAGMENTED
  c1-c2    1 run   234 of 234  sound, identity

rank
  1. .../v077-mono-c0  236 slots
  2. .../v077-mono-c1  234 slots

set: copies=3 signature=0 clusters=2 flagged=0 refused=0 fragmented=2 \
     slots=236 decided=yes best=corpus/incoming/op-duplicates/v077-mono-c0
```

The `pairs`/`clusters`/`rank` tables go to stderr for a person; the single
`set:` line goes to stdout with fixed field order and no line wrapping, so
`scripts/duplicate-survey.sh` can collect nineteen of them into a `notes.md`
table exactly the way `scripts/margin-survey.sh` collects `stats:` lines today.
That is a discipline this repository already has, and reusing it costs no new
serialisation format that could drift from the table beside it.

### Field order of the `set:` line

`copies signature clusters flagged refused fragmented slots decided best`

- `signature` — how many copies were removed by the signature step, before any
  alignment.
- `clusters` — distinct clusters after identity collapse, **all of them**,
  including any whose copies were flagged. The field states what the clustering
  found; what was kept out of the ranking is what `flagged` is for, so a set of
  two copies refusing each other prints `clusters=2 flagged=2`.
- `flagged` — copies that are not this volume: the ones that refused against
  every other copy, or, where a reference edition was given, the ones that
  paired nothing with it.
- `refused`, `fragmented` — pair counts by verdict, over the pairs actually
  solved.
- `slots`, `best` — the top-ranked copy, or `-` and `-` when nothing was ranked.
  **`best` is last**, because a corpus path can contain a space and no field
  after one would survive being split on whitespace.
- `decided` — `yes` when the ranking has a unique winner, `no` otherwise.

### Neither command moves, renames or deletes anything

`duplicates` reports and stops. Nothing in this workspace touches the library,
and §2's own summary of the One Piece run — "with 2 mislabelled files flagged
rather than silently ranked" — is an argument for a tool that hands a person a
decision, not one that acts on it.

It does write, and the distinction is worth keeping: every cluster
representative is fingerprinted through `cached_edition` into `.taiyaku-cache`,
the same cache `align` fills, which is what makes all-pairs affordable. Nothing
is written where the copies live.

## Decisions

### The signature is over uncompressed page sizes, not files on disk

Per copy: page count, total uncompressed bytes, and the SHA-256 of the sorted
list of page sizes. Only the digest decides; the other two are carried beside it
for a person to read, and nothing prints them today — `inventory`'s signature
column reports the verdict the digests reached, not the figures behind it.

**Over pages rather than over files**, so `sidecar.md`'s "How a member becomes
pages" decides what counts, and a `ComicInfo.xml` or a `Thumbs.db` does not
shift the signature. **Uncompressed rather than on-disk**, so a directory of
loose images and a `.cbz` of the same scan sign identically — packaging is
exactly what this step is meant to see through.

Both come out of a listing: `metadata().len()` per page for a directory, and the
zip central directory's uncompressed size per entry for an archive. No bytes are
read and nothing is decoded, which is the property that lets `inventory` run over
a NAS and decide what to copy.

### `inventory` claims sizes; `duplicates` confirms with a content hash

A matching signature says the two copies have the same page count and the same
page sizes. It does not say they hold the same bytes. `notes.md` treated the
three signature-identical One Piece sets as settled, and that was a reasonable
inference in a shell session; it was an inference.

`inventory` runs where reading the bytes is the cost being avoided, so it claims
only what a listing supports and prints `identical` meaning "identical
signature". **`duplicates` runs where the files are, so it confirms.** When two
copies share a signature it hashes both copies' *page bytes* with
`signature::content_digest` — a read, no decode — and only then reports identity
and skips the alignment, and the fingerprint pass with it. That is far cheaper
than the solve it replaces, and it turns the protocol's one assumption into a
measurement.

Page bytes rather than `taiyaku_read::member_digest`, which hashes an archive's
file bytes and a directory's page bytes: those disagree for one scan filed two
ways, which is exactly the case a signature match is most likely to be
reporting, so it would refuse to confirm what it was called about.

A signature match that fails the hash falls through to alignment, silently — the
two copies are simply solved like any other pair. It has never been observed; if
it ever is, it is worth a line in `notes.md`, and the command would have to say
so before it could get there.

### Identity is `paired == |A| == |B|`

Two copies are the same edition when every slot on both sides is paired.

**This is necessarily one run**, so run count is not tested separately: paired
runs are 1:1, so full coverage on both sides forces a constant offset, and an
offset that never changes is one run. The definition is stated as coverage
rather than as `runs == 1` because coverage is the property that matters and run
count is a consequence — and if a solve ever reported full mutual coverage
across several runs, that would mean run compression is broken, which is worth
finding out rather than papering over. The corpus check asserts one run
alongside the coverage for exactly that reason.

**The definition is what makes Chainsaw Man's base selection come out right.**
`notes.md` measured `v01` against `v01b` at "2 runs, 198 of 198, 0 unmatched" —
full coverage of A, one page on B that A never reaches. Under this definition
that is not identity, the two stay distinct, and `v01b` wins the rank on its
extra page. Collapsing them would discard the difference the choice is about.

> **A discrepancy in `notes.md`, recorded rather than smoothed over.** The
> Bocchi section calls `v01-02b/１巻` (124 pages, `raw-v01-src-krc` in the
> corpus) and `v01-04/第01巻` (125 pages, `raw-v01-src-hamaji`) "the identity
> case again", measured at "1 run, 124 of 124, nothing unmatched". That is the
> same shape as the Chainsaw Man pair the same document keeps distinct — full
> coverage of A with a page left over on B. So `notes.md` uses "identity" two
> ways, and this design uses only the stricter one.
>
> Which answer the command gives for that pair depends on something neither
> document measured: whether `krc`'s 124 pages expand to 125 slots, in which
> case the two are identity after all. **The corpus check does not assert it in
> advance.** It records the slot counts and the cluster outcome on its first
> run, and the observed value is pinned then — the way every other number in
> this repository was arrived at.

### One solve per unordered pair

`n` copies cost `n(n-1)/2` solves rather than `n(n-1)`, leaning on the symmetry
`status.md` records as measured across five pairs in two series. Identity is
readable from one direction: `paired_slots(runs)` counts A's paired slots and
B's alike, so `paired == |A| == |B|` needs no second solve.

Fingerprints come from `cached_edition` against `.taiyaku-cache`, so `n` copies
cost `n` fingerprint passes and not `n(n-1)/2`. That is what makes all-pairs
affordable at all, and it is why the protocol's expensive-sounding step 2 is not
expensive.

### Spreads always split; there is no `--split-spreads` flag

One Piece v092 is 200 images in one copy and 101 in the other, and comes out 1
run, 200 of 200 slots — but only through slot expansion, and *only on c0's
side*: c1's 101 images all detect as spreads and expand to 202 slots, so two of
them are never reached and the pair is containment rather than identity.
`notes.md` draws the conclusion directly: "Page counts are a property of
packaging. Slot counts are a property of the work." A duplicate command that
compared pages would get its own headline case wrong — it would see 200 against
101 and call them unrelated — so the flag `align` carries as a prototype option
is not offered here.

`--ltr` stays, because a spread's halves order by reading direction, and
direction matters precisely when one copy joins a spread the other splits.

### Clusters come from union-find, and non-transitivity is reported

Union-find over the identity pairs. Every pair is solved anyway, so if
`A ≡ B` and `B ≡ C` while `A ≢ C`, that contradiction is sitting in data already
in hand. The command prints it and continues with the merged cluster rather than
failing, because the merge is still the best available answer and the
contradiction means one of the three alignments is wrong — which is a thing to
go and look at, not a reason to refuse to report.

### Refused against everything is flagged, not ranked

A copy whose every pair comes back `Refused` is not this volume. That is the
Bocchi *Gaiden* file and the One Piece Starter Book, and §2 counts flagging them
among the run's results: "with 2 mislabelled files flagged rather than silently
ranked".

Flagged copies are reported in their own section and excluded from the ranking.

**With two copies and a refusal between them, both are flagged and nothing is
ranked.** Neither can be preferred, because nothing in the set says which one is
the impostor. `--against` is what resolves it, and the output says so rather
than leaving the person to work out why the rank section is empty.

**How it resolves it: where there is a reference edition, the reference
decides.** Every cluster representative is aligned against it, including ones
the within-set rule flagged, and a copy that pairs nothing with the reference is
not this volume whatever it did within the set — while a copy that does pair
with it is this volume even where it refused every other copy. That is the
mislabelled-file case exactly: the impostor refuses the real scans and the real
scans refuse it back, and only something from outside the set can say which is
which. Without a reference the within-set rule is the whole of it, and the hint
asking for one prints only there.

A copy that refuses against some copies and pairs with others is kept in the
ranking, with its refusals visible in the pairs table, because a refusal against
one member of a set that otherwise agrees is more likely to be a solver result
worth looking at than a claim about the file.

### Ranking is slots first, and a tie is reported as a tie

`notes.md` settles the ordering: "Pages and pairings measure content; runs
measure how cleanly the correspondence holds. Content should win, because a
missing page cannot be recovered from a cleaner alignment of the pages that
remain."

- **Without `--against`:** cluster representatives ranked on slot count alone. A
  tie at the top prints as a tie — two rows carrying the same slot count — and
  sets `decided=no`. No tiebreak is invented: within-set cleanliness is
  symmetric and near-useless with two clusters, and a statistic that decides
  nothing should not be dressed up as one that does.
- **With `--against`:** each representative is aligned once against the reference
  edition and ranked on slots, then paired slots, then run count — the Chainsaw
  Man table, reproduced.

One alignment per representative, not per copy: an identity cluster's members
are the same content by measurement, so aligning both against the reference
would spend a solve to produce the same answer twice.

**A cluster's representative is whichever of its members came first in the
argument list.** Members of one cluster have equal slot counts by the definition
of identity, so no member is a better representative than another and the choice
only has to be stable and explainable. The whole cluster is printed beside the
rank, so which one was picked is never hidden.

## Where the code goes

`main.rs` was 1,328 lines when this was written. Two commands inline would make
that worse, so the logic lives beside it and `main.rs` gains two match arms and
two usage lines.

| Where | What | Why there |
| ----- | ---- | --------- |
| `taiyaku-read` | `member_page_sizes(path) -> Result<Vec<u64>, String>` — uncompressed sizes in reading order, no bytes read | The same walk as `member_page_names`, and which entries are pages is this crate's decision to make |
| `taiyaku-cli/src/signature.rs` | `Signature`, its digest, and the content hash that confirms a match | Shared by both commands, one job |
| `taiyaku-cli/src/duplicates.rs` | clustering, ranking, flagging, both output forms | |
| `taiyaku-cli/src/inventory.rs` | the walk, the candidate rule, the name parse, its table | |

Formatting functions return strings rather than printing them, the way
`stats.rs` does and for the reason its module comment gives: the `set:` line is
the interface a survey script depends on, and a format nobody can test is a
format that drifts.

### Argument parsing

Copies are `--`-separated groups, as in `topology`. Unlike `topology`,
`duplicates` has flags, and `--against` itself begins with `--`. **Group
collection therefore stops at the first argument that starts with `--` and is
not exactly `--`.** So `taiyaku duplicates a -- b --against ref --ltr` is two
copies, one reference and one flag. `--against` takes members the way
`split_editions` already takes the b-side: everything up to the next flag.

Each group's members are sorted with `taiyaku_read::natural_cmp` at the point of
collection, for the reason `split_editions`'s doc comment gives — a shell glob
yields `c10.5, c10, c1, c2` and a member list is a page sequence.

## Testing

### Five gated corpus checks against four claims

Gated on `TAIYAKU_CORPUS` and skipping cleanly without it, joining the three
standing checks `scripts/check-all.sh` already runs with `--nocapture`. The
material is in the corpus today; `corpus/README.md` describes
`incoming/op-duplicates` as 34 copies covering 16 duplicated volumes and names
v077 as the identity pair and v092 as the containment pair a file listing gets
wrong — the second of those being the correction this document's own claim
needed.

**1. All-pairs beats anchoring on the first copy.** `v077-mono-c{0,1,2}` —
234, 234 and 214 pages. Exactly one identity cluster, of size two, containing
the 214-page copy; the other 234-page copy distinct. This is the check that goes
red if anyone rewrites step 2 as "each copy against copy 0", which is the mistake
`notes.md` records making and which reported v077 as three mutually unrelated
files. Asserted alongside it: the identity pair is one run, per the definition
above.

**2. A completely different pagination is still recognised.** `v092-mono-c{0,1}`
— 200 pages against 101. Measured: 200 slots against 202, all 200 of c0's
paired, one run — containment, not the identity this document first claimed, and
two clusters with c1 first on its extra slots. Goes red if spread expansion or
the containment relation stops being load-bearing here.

**3. A file that is not the volume it claims is flagged, not ranked.** Bocchi's
`raw-v01-src-{hamaji,krc,mangazip}` — 125, 124 and 185 pages. `mangazip` is the
*Gaiden* spinoff. Asserted: it refuses against both others, is flagged, and does
not appear in the ranking; the two survivors are ranked with the larger slot
count first. **Not asserted in advance:** whether `hamaji` and `krc` come out as
one cluster or two — see the discrepancy recorded above. The first run prints
their slot counts and the cluster outcome, and the plan pins the observed value
at that point. **Pinned since:** 125 slots against 124 with all 124 paired, so
`hamaji` *contains* `krc` and containment does not merge — three clusters, one
of them the flagged *Gaiden* file.

**4. Ranking against a reference reproduces base selection.** Chainsaw Man's
`raw-v01-src-{a,b,c}` and `raw-v01-colour` — 199 pages for `src-b`, 198 for the
other three — with `--against en-v01.cbz`. Asserted: `src-b` ranks first, on the
extra page. **Established by the first run rather than asserted:** which of
`src-a` and `src-c` pair as an identity cluster. `notes.md`'s table says two of
the three scans are the same pagination and only one of them needs aligning
against anything else, and the 198-page pair is the candidate — but the mapping
from that document's names to the corpus's is an inference from page counts, so
it gets measured before it gets pinned. **Pinned since:** `{src-a, src-c}` at
198 slots each, with `src-b` and the colour edition each standing alone.

**5. The inventory finds every set.** `taiyaku inventory
corpus/incoming/op-duplicates` — 16 volume numbers over 34 copies, every one
duplicated. This exercises the name parse against real names, including
`v095-colour-c0` parsing to 95 rather than to 0 or to 95-colour-0.

That is five checks against four claims because claim 1 splits cleanly into two
volumes measuring different things, and running them as one test would report a
single failure for two unrelated regressions.

### Unit tests, on fixtures rather than the corpus

Roughly fifteen, in the manner of `stats.rs`'s — pinning formats verbatim and
behaviours by construction:

- **Signature.** Same page sizes in a different on-disk order produce the same
  digest; one different size produces a different one; a directory and a `.cbz`
  holding the same pages sign identically. That last one is the packaging
  independence the whole step rests on.
- **`member_page_sizes`.** Same length and same order as `member_page_names`,
  and each size equals the byte length `read_edition` returns for that page.
  This is what pins "uncompressed, in reading order" — the property the
  directory-versus-archive case depends on.
- **Identity.** Synthetic run lists: `paired == |A| == |B|` is identity; one
  extra slot on B is not, which is the Chainsaw Man distinction as a test.
- **Clustering.** The v077 shape — `c1 ≡ c2`, `c0` distinct — from run lists
  rather than files. Non-transitive identity produces the warning and still
  merges.
- **Flagging.** All pairs refused means flagged and excluded; two copies with a
  refusal between them flag both and rank nothing.
- **Ranking.** Slots first; a tie without `--against` sets `decided=no`; with a
  reference, slots then paired then runs.
- **The `set:` line**, asserted verbatim, and its `best=-` `slots=-` form.
- **The name parse.** `v069`, `第69巻`, `069`, `--pattern 'v{}'`, and
  `ONE PIECE v077 (2015)` parsing to 77 rather than 2015. A name with no digits
  lands in the `no volume number` group rather than disappearing.
- **The candidate rule.** A nested volume whose chapter subdirectories are each
  under the threshold is one candidate, not seven and not zero.

## Risks, named

1. **The volume-number parse is a guess and always will be.** Mitigated by
   printing it, by `--pattern`, and by the two commands being separate — a wrong
   grouping is corrected by handing `duplicates` the right paths directly.
2. **The candidate rule can swallow a folder of non-volumes** when nothing
   inside it reaches the threshold. Visible in the printed page count; not
   otherwise detectable without knowing what the collection is.
3. **One root, one series.** Overlapping volume numbers across two series under
   one root group wrongly. Not addressed, because nothing in the corpus measures
   it.
4. **A signature collision across genuinely different scans** would need every
   page size to match exactly. `duplicates` confirms with a content hash, so the
   collision costs a wasted hash rather than a wrong answer. `inventory` cannot
   confirm and says only `identical signature`.
5. **Ranking on slots prefers the copy with more material, including material
   that is not the volume** — a scan carrying extra ads or a bonus insert
   outranks a clean one. `--against` is the answer, and without it the command
   reports the slot counts it ranked on rather than only the winner.

## Shape of the work

Roughly 700–900 lines including tests. Larger than M4, comparable to M3.

1. `member_page_sizes` in `taiyaku-read`, with its two tests.
2. `signature.rs` and its tests. Nothing else depends on the commands existing.
3. `duplicates.rs`: parsing, signature step, all-pairs, clustering, flagging,
   ranking, both output forms.
4. `inventory.rs`: the walk, the candidate rule, the parse, its table.
5. `main.rs` wiring and usage lines.
6. The five gated corpus checks, and `scripts/duplicate-survey.sh`.
7. Docs: `status.md` §2 rewritten as landed with the command's shape;
   `notes.md` recording what the five checks measured, including the two
   outcomes this document declines to predict; `corpus/README.md` gaining the
   `duplicates` invocations beside the `align` ones it already lists.

## What this leaves open

- **Series-level identity is still manual**, which `status.md` already records
  as a known gap. `inventory` groups by volume number within one root; it does
  not decide that two roots are the same series.
- **Nothing acts on the answer.** A person still moves or deletes the losing
  copies. That is deliberate and there is no plan to change it.
- **The art-book case is untouched.** Linking art book pages to the manga pages
  they came from wants nearest-neighbour search and a crop-tolerant hash, which
  `status.md` records as a different problem from the DP. A duplicate set of two
  art books would be ranked on slots like anything else, and the run structure
  would be fragmented in both directions.

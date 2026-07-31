# Making hub-and-spoke real — design

**Date:** 2026-07-30
**Status:** design agreed, nothing built

## Summary

Hub-and-spoke is designed in `sidecar.md`, validated across 4,815 page
comparisons, and has a `compose()` in the core. It is used by exactly one
command — `topology`, a measurement tool. Nothing a reader can reach composes
anything.

Two halves close that:

1. `align --write` adds a spoke to an existing sidecar instead of only minting a
   fresh two-edition file.
2. `resolve` answers through the hub, using the `compose()` that already exists
   and is already measured.

## Why now

The primary development corpus is Go-Toubun volume 1 with **five** editions, and
there is currently no way to produce a sidecar describing them, nor to resolve
through one if you hand-wrote it.

- `align --write` hardcodes two editions — the `editions: [...]` literal in
  `main.rs` — with `hub: a_name`. `sidecar.md` calls that "a degenerate star".
- `resolve` matches direct edges in the stored direction only (`sidecar.rs:136`).
  No stored A→B edge means no answer, and a B→A question is not answered by a
  stored A→B edge either.

`lookup` already apologises for this in as many words: *"but not by a path this
lookup can walk"*. This is the reader's actual blocker, and building the reader
first and discovering it is the expensive order.

## The measurement that settles the topology question

**The solver is symmetric on the pairing relation.** Five pairs solved in both
directions, comparing every paired run against its transpose:

| Pair | Paired runs each way | Result |
| ---- | -------------------- | ------ |
| Go-Toubun raw-mono ↔ en-mono | 3 | identical transposed |
| Go-Toubun raw-mono ↔ en-fan | 8 | identical transposed |
| Go-Toubun en-fan ↔ en-color | 11 | identical transposed |
| Kaguya raw-mono ↔ en-mono | 4 | identical transposed |
| Kaguya en-fan ↔ en-color | 13 | identical transposed |

Only the one-sided gap runs differ, which is by construction: a gap records
pages of the `from` edition with no counterpart, and each direction has its own.

**This decides how many alignments a sidecar stores.** `topology` composes
`solve(spoke, hub)` with `solve(hub, spoke)` — two independent solves, which is
why it reports eight alignments for five editions. A sidecar stores one edge per
spoke and must invert it to walk the first leg. Had inversion been lossy, the
measured 1.7% would not transfer and the star would need 2(N−1) stored
alignments. It is not, so **N−1 edges is enough and the measured loss carries
over to the shipped path.**

Worth recording because the code says otherwise on paper: `calibrate` is
asymmetric. `signal` is the median of the best match *per page of A*, so
swapping the sides changes it whenever the editions differ in length, and with
it the floor. Go-Toubun raw-mono against en-fan is 195 slots against 240 — the
widest mismatch in the corpus — and the DP's output does not move. The
asymmetry is real and does not reach the result on any material measured.

## 1. Core gets a `Route`

New module `crates/taiyaku-core/src/route.rs`. `resolve` moves out of
`sidecar.rs`, which is 961 lines already, and gains a companion that says how it
got there:

```rust
/// Which path a lookup walked.
pub enum Via {
    Identity,
    Direct,
    Inverted,
    Hub(String),
}

/// The slot-indexed correspondence between two editions, and its provenance.
pub struct Route {
    pub via: Via,
    runs: Vec<Run>,
    from_slots: Vec<Slot>,
    to_slots: Vec<Slot>,
}

pub fn route(sidecar: &Sidecar, from: &str, to: &str) -> Option<Route>;

impl Route {
    pub fn resolve(&self, page: u32) -> Option<Slot>;
}

/// B→A from A→B. Paired runs swap sides; gaps drop, because a page with no
/// counterpart in B says nothing about B.
pub fn invert(runs: &[Run]) -> Vec<Run>;
```

`resolve(sidecar, from, to, page)` keeps its signature, as
`route(sidecar, from, to)?.resolve(page)`. It has exactly one call site outside
the core — `main.rs:391` — and every existing test keeps compiling.

**The route is an object rather than a flag on `resolve` because composing is
not free.** It builds two slot tables and a composed run list. At 5 runs and one
page that is nothing; a reader paging through a 195-page volume in a browser
would rebuild it 195 times. Building it once is the difference between a lookup
and a scan.

Search order:

| Case | `Via` |
| ---- | ----- |
| `from` or `to` names an edition the document does not define | `None` |
| `from == to` | `Identity` |
| A stored `from → to` alignment | `Direct` |
| A stored `to → from` alignment | `Inverted` |
| Otherwise, both legs resolvable to and from `hub` | `Hub(hub)` |
| Any leg missing, or `hub` names no edition | `None` |

The existence check comes first, before identity, for the reason `trust`
already orders them that way: `route("nosuch", "nosuch")` must be `None` rather
than a confident identity over an edition that is not there.

Each leg of a composition is itself found by the `Direct`/`Inverted` rules, so a
star storing only `hub → spoke` edges resolves every ordered pair. `Identity`
is answered before any alignment is consulted, matching `trust`: an edition
corresponds to itself as a property of the edition, not of a run list that need
not exist.

`Via::Hub` names only the hub, not which of its two legs were inverted. That is
deliberate — "composed through `raw-mono`" is the fact a reader acts on, and in
a pure star every first leg is inverted, so reporting it would be noise on every
line.

**`invert` sorts its output.** The solver is monotonic, so swapping the sides of
a sorted run list yields a sorted one; a hand-edited file need not be monotonic,
and nothing else in the format enforces it. `counterpart` scans linearly and
does not require order, so this is for the readability of composed output rather
than for correctness — but an unsorted list makes `compress` emit one run per
page, which would make a composed alignment look fragmented when it is not.

## 2. `Sidecar::merge_spoke` — the rules live in core

```rust
impl Sidecar {
    pub fn merge_spoke(
        &mut self,
        hub_side: &Edition,
        name: &str,
        spoke: Edition,
        alignment: Alignment,
    ) -> Result<Merged, String>;
}

/// What a merge did, so the caller can say so rather than guess.
pub enum Merged {
    Added,
    Replaced,
}
```

`align` reads the file, solves, describes both editions, calls this, and writes.
The validation is therefore unit-testable without archives, and a browser reader
inherits the same rules rather than reimplementing them.

What it enforces:

1. **`work.reading` must match.** It decides which half of a joined spread is
   the earlier page, so a mismatch silently misfiles every spread in the file.
   Fatal.
2. **The a-side edition is the sidecar's `hub`.** This is the answer to the open
   question: a third edition is solved against the hub, not against whichever
   edition you had open. Because the file names its own hub, `--name-a` becomes
   *optional* on a merge and is an error if it names anything else.
3. **The a-side's member `sha256` list, in order, equals the stored hub's.**
   Paths are relative to the sidecar and the CLI's are relative to the working
   directory, so paths cannot be compared; content hashes can, and order matters
   because it defines the flattened page sequence. A mismatch means the archives
   moved under the alignment — error, and re-solve the work.
4. **An existing spoke name is replaced**, edition and edge together, and the
   command says so. Re-solving a spoke after fixing its archive is ordinary, and
   refusing it would make the ordinary case the annoying one.
5. **`hub` is never rewritten.** Changing the hub is a re-solve of the whole
   work, not a merge.

`--direct` relaxes rule 2 and nothing else: the a-side may be any edition the
document already defines, and the edge is stored `a → b`. `--name-a` is
*required* with `--direct`, for the same reason it is optional without it —
there is no longer a hub to fall back on. This is the mitigation
`sidecar.md` already designs — *"Direct edges are permitted for pairs where
composition demonstrably loses"* — and the measurements name the pair that has
earned one, the English Full Colour release against the fan translation at 3.5%
to 3.7%. Rules 1, 3 and 5 still apply.

**`--write` onto an existing file now merges rather than clobbering.** Today it
is a bare `std::fs::write`. Merging is strictly the safer behaviour, and the
command prints what it did:

```
merged taiyaku.json: added `en-fan` | 3 editions, 2 alignments
```

Only the `--split-spreads` path writes a sidecar, as today; the plain path still
returns early.

## 3. `lookup` names the path it walked

```
$ taiyaku lookup taiyaku.json en-mono en-fan 45
Go-Toubun no Hanayome | 5 editions | reading RightToLeft
  alignment en-mono -> en-fan: FRAGMENTED, average run 26.9, composed through raw-mono
  en-mono   45 -> en-fan   85
```

That is the real output, not an illustration. The verdict is `FRAGMENTED`
because composition is only as good as its weaker leg and `raw-mono -> en-fan`
is the weak one; the line reports that rather than inheriting the `sound` of the
other leg. Walking the legs by hand agrees: en-mono 45 → raw-mono 47 → en-fan 85.

`Inverted` prints `from the stored raw-mono -> en-mono edge`. `Identity` prints
`identical edition`. `Direct` is unchanged from what it prints today.

Saying which path was walked is the same principle as printing the verdict on
every call rather than only a bad one: a composed answer is weaker than a direct
one, and a reader who cannot tell them apart has been told less than the
document knows. It also makes the 1.7% legible — a `no counterpart` under a
`composed through` line is the predicted hub-gap case, not a broken tool.

The current "not by a path this lookup can walk" branch is **deleted**. After
this change `resolve` and `trust` agree about which paths exist, so it is
unreachable.

## 4. What is deliberately not changed

**`Sidecar::trust` keeps `min` over the two legs.** Now that the composed runs
exist it could judge those instead, and there is a real argument for it — the
document's own rule is that the runs are the authority. There is also a real
argument against, and it is not a matter of taste:

Composition can *merge* runs. If leg 1 pairs `a:0-1 → hub:0-1` and
`a:2-3 → hub:10-11`, and leg 2 pairs `hub:0-1 → c:0-1` and `hub:10-11 → c:2-3`,
the composed result is `a:0-3 → c:0-3` — one clean run out of two fragmented
ones. Judging the composed runs would then report `sound` where `min` reports
`fragmented`, and which of those is correct is an open question rather than an
obvious one.

So `trust` is untouched, and the corpus run in §5 reports how often the two
disagree on real material. If the answer is never, the question closes for free.
That is a follow-up decision, not this piece of work.

## 5. Verification

**Unit tests**, in `route.rs`:

- `invert` round-trips a paired run list, and drops gaps.
- `resolve("en-mono", "raw-mono", 45)` returns raw page 47 against the real
  Go-Toubun document already embedded in `sidecar.rs`. This is the inverse of the
  existing forward assertion and pins the behaviour change below.
- A three-edition fixture composes, shaped after the existing
  `trust_composed_through_the_hub_takes_the_worse_edge`.
- A page landing on a hub gap loses its counterpart, mirroring the existing core
  test of the same name — the 1.7%, asserted at the sidecar layer.
- Identity resolves to itself, including for a spread page.
- A missing leg returns `None` rather than a partial answer.

**Merge tests**, in `sidecar.rs`, all without archives: hub mismatch refused;
reading-direction mismatch refused; changed hub `sha256` refused; existing spoke
replaced; `hub` unchanged after a merge; `--direct` edge stored in the named
direction.

**Corpus test**, gated on `TAIYAKU_CORPUS` and skipping when unset so CI stays
green without the archives. This is the standing check `notes.md` asks for under
"Validate it against all-pairs, and keep checking" and which does not exist. It
builds the five-edition Go-Toubun sidecar through the merge path, then for every
ordered spoke pair compares `route(...).resolve(p)` against a direct solve, page
by page.

The thresholds are Go-Toubun's own measured figures, not the corpus average:
`topology` reports **0.00% disagreement and 1.90% loss** for this series — the
1.7% quoted elsewhere is the mean across both five-edition series, and asserting
it here would be tighter than the material supports. So: disagreement exactly 0,
loss no worse than 1.90%. The test shares `.taiyaku-cache`, without which each
edition costs seconds rather than milliseconds.

**`topology` re-run** on both five-edition series, with one new column:
composed verdict against `min` of the two legs, feeding §4. `notes.md` records
the result whichever way it falls.

**Documentation**, because three files currently describe the unbuilt state:

- `sidecar.md` — the paragraph beginning *"Resolving through the hub is not
  implemented"* is rewritten, and the `lookup` example replaced.
- `notes.md` — §"Result: the star holds" gains the symmetry measurement above
  and the shipped-path figures.
- `status.md` — the commands table and "What is known to be wrong or missing".

## Behaviour changes a reader would notice

Stated plainly rather than discovered from a failing test.

- **`resolve` answers B→A from a stored A→B edge.** The existing assertion
  `resolve(&s, "en-mono", "raw-mono", 0) == None`, commented *"an edition pair
  with no alignment recorded"*, now returns a page. The comment was describing a
  limitation, not an intent.
- **`resolve` answers spoke-to-spoke.** Previously `None` on any pair without a
  stored edge.
- **`--write` merges into an existing file** instead of overwriting it.
- **No `taiyaku` version bump.** Nothing about the stored format changes: a
  three-edition document is already legal, and every field keeps its meaning.
  What changes is which questions this library can answer about one.

## Known limits

**A composed run carries no `margin`.** `compose` produces runs, not
confidences, and the two legs' margins are not combinable — the minimum over a
composed run is not the minimum of the legs' minima. Composition is therefore a
read-time operation only, never written to disk, and a reader wanting per-page
confidence on a composed path has to recompute from fingerprints, exactly as
`sidecar.md` already says for a direct one.

**Rule 3's `sha256` check will fire on any corpus file re-extracted since its
sidecar was written.** That is a true positive — the alignment genuinely is
against a different file — but it may be noisy the first time it meets the
corpus.

**Hub choice is still declared, not measured.** `notes.md` argues hub selection
should minimise total loss across spokes, and it remains a convention here: the
merge keeps whatever hub the file names. Making that a measurement is a separate
piece of work and is not blocked by anything in this one.

# Recording the alignment verdict in the sidecar — design

**Date:** 2026-07-30
**Status:** design agreed, nothing built

## Summary

A sidecar written from a worthless alignment is indistinguishable from one
written from a good alignment. The format records how two editions correspond
but says nothing about whether the correspondence is believable, so every reader
that consumes one has to either re-derive that from the archives — which is what
the sidecar exists to avoid — or assume the answer is good.

This adds a `quality` block to each alignment, and a `Sidecar::trust` query that
composes the verdict along the hub path.

## Why now

Measured two days ago and recorded in `notes.md` under "Twenty-two needles":
seven short unrelated pairings clear calibration and pair **100% of their pages
with nothing unmatched**. Neither calibration statistic can reject them — a One
Piece art book against Go-Toubun beats four confirmed-good pairings on margin and
seven on separation. Only run structure separates, and `verdict` already computes
it.

The same defect has now appeared twice at different layers. `verdict` was
computing `Fragmented` correctly and `report_summary` was not being called in the
`--split-spreads` path, so nobody saw it; that was fixed. The sidecar writer has
the identical shape of bug: the verdict is computed during `align` and then not
written. Here is what a sidecar from the XCalendar needle contains today — 21
runs across 26 slots, average run 1.2, and nothing that says so:

```
alignment keys: from, to, by, solver, runs
solver: { hash, floor }
```

## The format change

`quality` is a new optional block on `Alignment`, a sibling of `solver`:

```json
{
  "from": "raw-mono", "to": "en-mono", "by": "solver",
  "solver": { "hash": "dhash-16", "floor": 0.687 },
  "quality": { "verdict": "sound", "slots": 195, "paired": 193, "avg_run": 64.3 },
  "runs": [ ... ]
}
```

`verdict` is `"sound"`, `"fragmented"` or `"refused"`, matching the existing
`Verdict` enum and serialised lowercase as `Source` already is.

**It belongs on `Alignment` rather than inside `SolverInfo`, because the verdict
is a function of the runs and not of the calibration.** Three consequences follow
from that placement, and each is a reason for it:

- A `by: "hand"` alignment has no `solver` block but does have runs, so it can
  and should carry a quality block.
- A refused pair can state `"verdict": "refused"` explicitly, rather than leaving
  a reader to infer it from an absent `solver`.
- The block stays meaningful if the solver is ever replaced.

The statistics are stored alongside the verdict rather than the verdict alone,
for the reason the format already stores a per-run `margin`: a summary is worth
having for readers that hold only the document. It also means the threshold that
produced the verdict is auditable — `MIN_AVERAGE_RUN` is 27.0 today and the
measure was runs-per-hundred a week ago, so a bare verdict would silently mean
different things in files written a week apart.

## `Sidecar::trust`, and why it recomputes

```rust
impl Sidecar {
    /// The worst verdict on the path from one edition to another, or `None`
    /// when the document does not say.
    pub fn trust(&self, from: &str, to: &str) -> Option<Verdict>;
}
```

**It recomputes from the runs rather than reading the stored `quality`.**
Everything `verdict` needs is already in the document: `runs` yields the paired
count and the average run length, and `Edition::slots()` yields the denominator.
Recomputing means the library's answer cannot go stale when someone hand-edits
runs, which is a hazard `sidecar.md` already raises for the `by: "hand"` case —
a corrected alignment must survive a re-solve, and an edited run list with an
unedited verdict beside it would be exactly the silent contradiction the format
is supposed to prevent.

The stored block is therefore a convenience, never a dependency. That makes a
stored-versus-recomputed disagreement a *detectable* condition: a hand edit that
was never re-solved. `verify --sidecar` is the natural place to report it. **Not
built here** — noted so the next person does not have to rediscover that it is
possible.

**Composition takes the worst edge.** Ordering is `Refused < Fragmented <
Sound`. A lookup from A to B resolves through the hub, so its trustworthiness is
bounded by the weaker of A→hub and hub→B. A direct edge, which the format
already permits, wins over composition when present.

**Absent `quality` returns `None`, and `None` is not `Sound`.** A sidecar written
before this change means *unknown*. A reader must not read silence as approval,
which is the whole lesson of the work that led here.

Since `trust` recomputes, "absent `quality`" means something precise: `None` is
returned when the *path itself* cannot be established, not when the block is
missing. Spelled out, so no two implementations differ:

| Case | Returns |
| ---- | ------- |
| `from` or `to` names an edition the document does not define | `None` |
| `from == to` | `Some(Sound)` — an edition corresponds to itself |
| A direct `from → to` alignment exists | its recomputed verdict |
| No direct edge, and both `from → hub` and `hub → to` exist | the worse of the two |
| No direct edge and either leg is missing | `None` |

Alignments are directional in the document, and a reader asking for `B → A` when
only `A → B` is stored should get an answer rather than `None`: the runs are
A-indexed, but the verdict is a property of the pairing, so an edge may be read
in either direction for trust purposes even though `resolve` needs the stored
direction to map pages. `slots` and `paired` in a stored block are always
A-side counts, matching how runs are indexed.

## `lookup` prints trust on every call

Not only on bad ones. A warning that appears sometimes trains people to skip it;
a line that is always present is read as part of the answer.

```
$ taiyaku lookup t.json raw-mono en-mono 47
Go-Toubun no Hanayome | 2 editions | reading RightToLeft
  alignment raw-mono -> en-mono: sound, average run 64.3
  raw-mono 47 -> en-mono 45 right half

$ taiyaku lookup needle.json cal op 3
not a real work | 2 editions | reading RightToLeft
  alignment cal -> op: FRAGMENTED, average run 1.2
  cal 3 -> op 141
```

`FRAGMENTED` is capitalised and `sound` is not, matching what `align` already
prints, so the shouting is reserved for the case that needs a human.

## What does not change

- **`resolve` keeps its signature.** `resolve(sidecar, from, to, page) -> Option<Slot>`.
  Trust is a property of the edge, not of the page, so returning it per-page
  would repeat one fact for every lookup.
- **No `taiyaku` version bump.** The change is additive, and a reader ignoring
  `quality` is exactly as wrong as it is today rather than newly wrong.
  `sidecar.md` reserves bumps for changes that make an old reader wrong.
- **`verify` and the HTML report** are untouched.
- **A refused pair still writes a sidecar.** That file is the identity oracle's
  output — a record that two editions were compared and do not correspond — and
  it now says so in a field instead of by omission.

## Testing

- A sidecar carrying `quality` round-trips, added to the existing round-trip test.
- A document without `quality` parses, and `trust` returns `None` for it — the
  forward-compatibility rule already has a test for unknown fields; this is the
  other direction.
- `trust` composes: a Sound edge and a Fragmented edge compose to `Fragmented`.
- `trust` recomputes rather than reading: a document whose stored `quality` says
  `sound` while its runs are confetti returns `Fragmented`. This is the test that
  pins the design decision, and it fails against any implementation that reads
  the stored value.
- End to end: the needle sidecar in the scratchpad resolves with `FRAGMENTED` on
  screen, and Go-Toubun v01's resolves with `sound`.

## Known limits, stated rather than discovered later

**`trust` needs the reading direction to recompute the slot denominator**, which
it takes from `work.reading`. A document whose `reading` is wrong therefore gets
a wrong denominator. The existing format already requires `reading` to be correct
for spans to mean anything, so this adds no new exposure, but it is a second
thing that silently depends on it.

**Nothing enforces that a stored `quality` is honest.** A hand-written sidecar
can claim `"verdict": "sound"` over confetti. `trust` ignores the claim, so this
library is not fooled; a reader that reads the JSON directly and believes it
would be. That is the cost of storing a summary at all, and the alternative —
storing nothing — was rejected because it pushes the computation onto every
reader.

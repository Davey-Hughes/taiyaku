# Status

Where the project is, what is settled, and what comes next. Updated 2026-07-30.

The detailed evidence for everything asserted here is in [`notes.md`](notes.md),
which is a working log rather than a summary — every claim below traces to a
measurement recorded there. The format is specified in
[`sidecar.md`](sidecar.md).

## Where it stands

**L0 works.** The aligner reads two editions, pairs their pages, and writes a
sidecar. It has been measured against eleven series and holds up on every case the
design anticipated, plus several it did not.

There is no reader yet. That is the next large piece of work and none of it has
started.

## What is built

Two crates, 36 tests, no dependencies beyond `image`, `zip`, `rayon` and `serde`.

`taiyaku-core` is the portable half — no I/O, so it compiles to WebAssembly:

| Module | Holds |
| ------ | ----- |
| `solve` | similarity, per-pair floor calibration, Needleman-Wunsch, run compression, confidence, verdict |
| `slot` | spread detection, slot expansion, reading direction |
| `sidecar` | the `taiyaku.json` format — parse, emit, resolve a page through an alignment |

`taiyaku-cli` is everything that touches the disk: archive and directory reading,
decoding, fingerprinting, the content-addressed cache, the HTML report.

Four commands:

```
taiyaku align  <a-member>... -- <b-member>...   [--write FILE] [--split-spreads] [--ltr]
taiyaku verify <a-member>... -- <b-member>...   [--out FILE] [--sidecar FILE]
taiyaku topology <hub>... -- <spoke>... -- <spoke>...
taiyaku lookup <taiyaku.json> <from> <to> <page>...
```

`align` takes several members on either side, which is how one volume aligns
against seven chapter files, or two tankōbon against one deluxe volume. `verify`
writes a contact sheet for looking at what the aligner decided. `lookup` resolves a
page through a written sidecar, which is the operation a reader performs.

## What is settled

Decided by measurement, not preference. Each of these was tried against an
alternative that lost.

- **dhash-16 over pHash.** 256 bits on luma, chosen on measured separation.
- **Margin over similarity as the confidence signal.** How far the best match beats
  the runner-up, not how good it looks in isolation.
- **A per-pair similarity floor**, calibrated from the pair itself rather than fixed
  globally. This is what refuses unrelated volumes.
- **Fragmentation as the quality signal**, read together with coverage — neither is
  sufficient alone.
- **Slots, not pages, as the addressed unit.** A slot is a whole page or one half of
  a joined spread. Spreads expand before solving, so the alignment stays strictly
  1:1 and the solver never learns they exist.
- **Reading direction is an input.** Monotonicity forbids reversing a pair, so it
  cannot be discovered.
- **Hub-and-spoke topology**, raw as hub, validated against all-pairs at ~0%
  disagreement and ~1.7% loss.
- **JSON for the sidecar**, with span notation (`7`, `4R`, `2-5`, `45R-46L`).
- **A content-addressed fingerprint cache** keyed on member SHA-256. 3.4 s → 189 ms.

## What has been proven against real material

Everything here is a measurement against the library, not a synthetic test.

| Case | Result |
| ---- | ------ |
| Same volume, raw ↔ English | sound across eleven series |
| Colour ↔ monochrome | costs little; less than removing text does |
| Joined spread ↔ split pages | recovered symmetrically via slots |
| Chapters → volume | handled by multi-member alignment |
| **2 tankōbon → 1 deluxe volume** (Monster) | 427 of 444 slots, 13 runs |
| **4 volumes → 1 deluxe volume** (Nausicaä) | 553 of 600 slots, 20 runs |
| **Letterboxed spreads** (Tongari v09) | 176 of 180 slots once trimmed symmetrically |
| **All-spread edition ↔ ordinary** (One Piece v092) | 200 of 200 slots, 1 run |
| Unrelated volumes | refused |
| **A mislabelled file** (Bocchi *Gaiden* filed as v01) | refused, and its real counterpart found |
| Art book ↔ manga | pairs everything and should not — see below |

## What is known to be wrong or missing

Stated plainly because each is a thing a reader would hit.

**A small needle in a large haystack always aligns.** When side A is much shorter
than side B, the DP inserts gaps in B freely, so A comes back fully covered no
matter how unrelated the two are. Coverage-based refusal does not work in that
direction; margins separate cleanly where counts do not. This is the gap the art
book experiment exposed, and it is the most important open correctness issue.

**Art books cannot be aligned and the solver does not say so.** Reading them works.
Linking their pages to the manga pages they came from is a different problem —
sparse, unordered, cross-composition — and wants nearest-neighbour search with a
margin threshold plus a crop-tolerant hash. Not the DP.

**No correction UI.** The design calls this part of the core, and it does not exist.
`verify` shows what the aligner decided; nothing lets you fix it.

**No reader.** Nothing consumes a sidecar except `lookup`.

**Series-level identity is manual.** Which two files are the same volume is decided
by the person running the tool.

## What comes next

Roughly in order. The first two are small and unblock things; the third is the large
piece.

### 1. Refuse on margins, not just counts

The needle-in-a-haystack gap is a correctness issue and everything else is built on
the refusal being trustworthy. Average run length is probably the better statistic
to expose than runs-per-hundred: measured, a genuine section ran 103 slots where
noise ran 1.8 to 2.1.

### 2. Duplicate resolution as a command

The protocol is worked out and was run by hand across 109 volumes. It wants to be a
command:

1. **Signature** — file count, total bytes, sorted size list. Settles exact
   duplicates without decoding anything.
2. **All-pairs alignment within the set** — not everything against the first copy,
   which hides clusters. Identity collapses copies differing only in packaging,
   including across different image counts.
3. **Rank what remains** by content first (slots, then pairings), then by how
   cleanly it aligns.

On the One Piece collection that took 19 duplicate sets down to 13 real choices,
with 2 mislabelled files flagged rather than silently ranked.

### 3. The reader

A PWA consuming the sidecar, with the correction UI as part of it rather than
after it. This is the multi-week piece and the point of everything above.

### Later

- **L1 geometry** — affine transform per pair, so a swap preserves zoom and pan.
- **L2 text regions** — bubble pairing by transformed centroid, which L1 makes
  arithmetic rather than a matching problem.
- **Art book linking**, if it is worth it — needs a different hash, and roughly one
  art book page in three has any manga counterpart at all.

## Corpus

Eleven series with both a raw and at least one English edition. Held on a NAS, never
committed, located at build time. Beyond the four the design started with, the
material now covers repagination at 2:1 and 4:1, editions published entirely as
joined spreads, letterboxed scans, archival TIFFs at 430 MB per page, three
independent scans of one volume, and at least two files that are not what their
names claim.

The awkward material is the valuable material. Every guard rail in the solver exists
because something in this corpus broke without it.

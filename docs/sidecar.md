# The sidecar format — draft

**Status: draft, with both open questions now settled.** Everything here is
settled by measurement against the corpus; see `notes.md` for the evidence behind
each decision.

`taiyaku.json` sits beside the archives it describes and records how the editions
of one work correspond to each other. It is the product: the reader here exists
to prove the format is useful, not to be the only thing that reads it.

Design goals, in priority order: human-readable, diffable in git,
hand-correctable, and forward-compatible so an L0-only reader can ignore L1 and
L2 fields it does not understand.

## Producing and consuming one

```
taiyaku align  <a>... -- <b>... --split-spreads --write taiyaku.json \
               --title NAME --name-a raw-mono --name-b en-mono
taiyaku lookup taiyaku.json raw-mono en-mono 47 48
taiyaku verify <a>... -- <b>... --sidecar taiyaku.json --out report.html
```

`verify --sidecar` takes the alignment from the file rather than the solver,
which is what makes a hand correction reviewable: edit the runs, render again,
look at what changed.

Fingerprints are cached under `.taiyaku-cache/`, keyed by each member's SHA-256,
so re-solving costs milliseconds rather than seconds — measured at 3.4 s cold
against 189 ms warm for one volume pair. The cache stores page dimensions and
hashes, including both halves of every landscape page, so the spread rule can
change without invalidating anything.

`lookup` exists to prove the format carries what was solved: it reads the
document with no archives present and no solving, and its answers match what
`align` printed. On Go-Toubun volume 1, raw page 47 resolves to the right half
of English page 45 either way.

## Shape

```json
{
  "taiyaku": 1,
  "work": {
    "title": "Go-Toubun no Hanayome",
    "reading": "rtl"
  },
  "editions": {
    "raw-mono": {
      "label": "Raw, monochrome",
      "members": [
        { "path": "Go-Toubun no Hanayome - v01.cbz", "sha256": "3f9a…", "pages": 195 }
      ],
      "spreads": []
    },
    "en-mono": {
      "label": "English official, Kodansha",
      "members": [
        { "path": "Go-Toubun no Hanayome - v01 (2018).cbz", "sha256": "c104…", "pages": 187 }
      ],
      "spreads": [4, 45, 46, 49, 90, 121, 152]
    }
  },
  "hub": "raw-mono",
  "alignments": [
    {
      "from": "raw-mono",
      "to": "en-mono",
      "by": "solver",
      "solver": { "hash": "dhash-16", "floor": 0.68 },
      "runs": [
        { "a": "0",     "b": "0",     "margin": 0.086 },
        { "a": "1" },
        { "a": "2-5",   "b": "1-4R",  "margin": 0.152 },
        { "a": "6" },
        { "a": "7-194", "b": "5-186", "margin": -0.133 }
      ]
    }
  ]
}
```

That is the real alignment of Go-Toubun volume 1, not an illustration. Five runs
for 195 pages.

## Why it is that short

The staircase of offsets that earlier passes recorded — −1, −2, −4, −5, −6, −7,
−8 — was **entirely an artifact of joined spreads having no representation**.
Once a spread is two slots, the volume is two front-matter gaps and then a single
constant offset from page 7 to the end.

That is the strongest argument for the slot model: it does not merely recover
lost pages, it makes the correspondence simple enough to read at a glance, which
is what "hand-correctable" was always supposed to mean.

## Definitions

### Work

`reading` is `"rtl"` or `"ltr"` and is **required**. It decides which half of a
joined spread is the earlier page, and the alignment cannot derive it: the solver
is monotonic, so a spread whose halves are listed in the wrong order simply loses
one of them. This is measured, not theoretical — listing halves in image order
rather than reading order cost exactly half the recoverable pages.

### Scope: one sidecar per work, not per volume

**Settled by testing rather than argument.** Three Go-Toubun volumes flattened
into one edition on each side, aligned in a single pass:

```
585 slots against 580, thirteen runs, 578 of 585 pages paired
```

Thirteen runs where solving the three volumes separately gives fifteen, because a
run merges straight across a volume seam when the offset does not change there.
Volume boundaries need no representation at all, and the edition model needed no
change to reach series scope.

Two consequences worth stating:

- **Chapter membership stops being a question.** If an edition is the whole
  series, nobody has to know which chapters constitute volume 5 — the alignment
  places them. The corpus twice showed declared membership to be wrong, and at
  series scope it simply does not matter.
- **A reader opens one file per work** rather than finding and merging fourteen.

The honest costs. A change to any member re-solves the work, though the
fingerprint cache is keyed per member so only the changed one is re-read. And a
sidecar for a multi-edition work cannot "sit beside the archives" in any
meaningful sense, because the editions live in different trees — `Manga/`,
`Manga (RAW)/`, `Manga (Chapters)/`. Paths stay relative to the sidecar, so
`../../Manga/…` is expected and fine; the natural homes are beside the hub
edition or a central directory holding one file per work.

### Edition

An edition is a named, ordered page sequence assembled by concatenating its
`members`. One raw volume may correspond to six chapter archives on the other
side; both sides flatten before anything else happens, so volume-against-chapters
is not a special case. A member is a volume, a chapter file, or a directory of
pages — an edition is however many of them the work spans.

- `path` is relative to the sidecar.
- `sha256` is over the member file, so a reader can tell the archive was replaced
  and the alignment is stale. This is not hypothetical: the library contains two
  files both claiming to be volume 1 of the same work, only one of which is.
- `pages` is the member's image count, which lets a reader detect a mismatch
  without opening it.
- `spreads` lists the indices, in the edition's flat page sequence, of pages that
  are joined double-page spreads. Detection is by aspect ratio and is cheap;
  storing it means a reader never has to re-derive it.

### Slots and run notation

A **slot** is one alignable unit: a whole page, or one half of a joined spread.
An edition's slot sequence follows from its pages and its `spreads` list, in
reading order. For `en-mono` above, whose page 4 is a spread, the sequence begins:

```
slot 0 → page 0
slot 1 → page 1
slot 2 → page 2
slot 3 → page 3
slot 4 → page 4, right half     (right first, because reading is rtl)
slot 5 → page 4, left half
slot 6 → page 5
```

Runs address slots, but are written in page terms so a human can read them:

- `"7"` — page 7, whole
- `"4R"` / `"4L"` — the right or left half of the spread at page 4
- `"2-5"` — a contiguous slot range, inclusive at both ends
- `"b": null` — the pages in `a` have no counterpart

Both sides of a paired run cover the same number of slots. That invariant is what
makes a run expressible as two endpoints instead of a list of pairs, and it holds
because spreads are expanded on **both** sides — an edition can join a spread the
other one splits, in either direction, and the corpus contains both.

`margin` is the **weakest** per-page margin over the runner-up anywhere in that
run. Similarity is deliberately not stored: a chapter divider that resembles all
nine of its siblings scores 0.98 and means nothing.

Taking the minimum makes a long run only as trustworthy as its worst page, which
is why the 188-page run above reports −0.133 while nearly every page in it is
comfortable. That is the conservative reading and the right one for "does this
run need a human", but it means **a run-level margin cannot shade individual
pages**. A report wanting per-page confidence has to recompute it from the
fingerprints, which the cache makes cheap; the stored number is a summary for
readers that have only the document.

A negative margin is kept rather than clamped. It marks a page where the solver
passed over a better-scoring candidate to keep the sequence monotonic — usually
correct, occasionally the thing most worth looking at.

### Alignments and the hub

`hub` names the reference edition. Every other edition aligns to it, and any A→B
lookup resolves by composing A→hub→B. This is validated rather than assumed:
across 4,815 page comparisons on two series, composition disagreed with a direct
solve four times, all in the weakest pairing in the corpus. Loss is about 1.7%,
in the safe direction — a reader sees "no counterpart" rather than a wrong page.

Direct edges are permitted for pairs where composition demonstrably loses. The
measurements say where to spend one: editions furthest from the hub lose 3.5% to
each other, editions sharing the hub's page count lose under 0.5%.

`by` is `"solver"` or `"hand"`. A hand-corrected run must survive a re-solve;
this is the field that makes the correction UI more than a viewer.

`note` is free text on a run, for whoever corrected it to say why:

```json
{ "a": "47-50", "b": "45R-46L", "by": "hand",
  "note": "solver split the spread; both halves check out" }
```

This exists because JSON has no comments, and it is the better answer anyway.
**A comment would not survive the next re-solve** — the writer regenerates the
file and any annotation floating beside the data is gone, in any format. A field
survives, is attached to the run rather than merely near it, and the verification
report can show it. Anything worth writing down about an alignment is data.

## Forward compatibility

A reader **must** ignore keys it does not recognise. L1 adds a per-run or
per-page transform, L2 adds matched text regions, and an L0-only reader consumes
neither. Version bumps of `taiyaku` are reserved for changes that would make an
old reader wrong rather than merely incomplete.

## Previously open, now settled

Both questions were tested against raws obtained for the purpose. Neither changed
the format.

**1. Gatefolds — resolved, keep `Half`.** Berserk was the strongest candidate
available: nine joined spreads in volume 1, more than any volume in the corpus,
in a series famous for elaborate spreads. Every one measures 3620×2560 against a
1810×2560 page — exactly twice, to the pixel. Tongari and Monster carry none at
all. Across 37 spreads now measured, **not one is anything but a clean double**,
so `spreads` stays a list of page indices and a slot keeps a left/right half.

If a gatefold ever turns up, the migration is still the one sketched before:
`spreads` becomes `[{ "page": 4, "parts": 3 }]` and a half becomes part *i* of
*n*. Worth noting the reason to keep waiting rather than generalise now — a
gatefold plausibly needs geometry rather than a fraction, and that is L1's
problem, not L0's.

**2. Omnibus editions — resolved, `members` needs no change.** Monster's English
Perfect Edition collects two Japanese volumes per book. Aligning the two Japanese
volumes as one edition of two members against the single English member:

```
416 of 444 pages paired, 6.5 runs per 100 pages
```

That is inside the band every correct pairing occupies. The member seam is
visible in the output as an offset change at exactly the right place — the runs
hold a constant +1 offset through page 216, gap for the ten pages of back matter
the omnibus drops, then settle at −14 for the whole of the second volume. Two
members on one side and one on the other needs nothing the format did not
already have, because both sides flatten before anything else happens.

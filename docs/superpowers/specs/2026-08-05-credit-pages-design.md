# Skipping scanlation credit pages — design

**Status: designed, not built. Revised 2026-08-05 after a prototype run against
the corpus contradicted three of its claims.** Settled in brainstorming on
2026-08-05. This builds §4 of [`../../status.md`](../../status.md), "Skipping
scanlation credit pages", on the measurement recorded in
[`../../notes.md`](../../notes.md) under "Scanlation credit pages are already
visible in the alignment".

Every figure quoted below is measured. Where this document expects a result it
has not measured, it says so and names the run that establishes it. **One number
this design turns on — the run-length bound — is deliberately not chosen here.**
A survey fixes it, and the design says what happens if no value fits.

> ## Revision: three claims the first draft got wrong
>
> The first draft of this document was written from `notes.md`'s account and
> committed before any of its arithmetic was run. Both signals were then
> prototyped against `corpus/gotoubun-v01`'s real sidecar and the CLI's real
> fingerprint cache, and twelve candidate pages were extracted and viewed. The
> corrections are made in place below as well as named here, and `notes.md`
> itself was corrected in the same pass — see its "Corrected 2026-08-05" section.
>
> **1. `c005.5`'s four leading pages are not credit pages, so the hole this
> document was built around does not exist.** The draft's headline finding was
> that `notes.md`'s shape rule missed four leading credit pages, and it added an
> `at_member_start` flag to `UnpairedRun` to reach them. Extracted, those four
> pages are the volume's own front matter: the tankōbon cover, the title page,
> the table of contents, and a decorative half-title. A reader wants them. The
> original rule was right to exclude them, and `at_member_start` would have added
> four false positives while claiming to fix a defect.
>
> **2. Signal 1 is stronger than either document said, and its weakness is
> recall.** Splitting unpaired runs at member boundaries gives four single
> unpaired pages at a member's final index, and **all four are credit pages** —
> including `c005` local 26, which `notes.md` had labelled "chapter end" and
> discarded. Precision is four of four. But `c000` local 43 is a credits page for
> a second group, `underdogscans + BOSEN SCAN`, and the solver **paired** it, so
> no unpaired-based signal can see it at all.
>
> **3. Signal 2 is a duplicate detector, and cannot classify or confirm.** Its
> two tightest matches in the edition, at Hamming distance 2, are the volume
> cover, which the release carries in two chapter files. Its correct hits — the
> three donation pleas — sit at 14 to 24, not near zero, because a near-black
> card has few strong gradients and re-encoding flips the weak ones. And it misses
> both staff-credit pages at distances 93 and 99, because each appears once. So it
> cannot be the classifier, and gating signal 1 on it would veto the two true
> positives signal 1 uniquely finds.
>
> One arithmetic correction with no design consequence: `en-fan` is 235 pages and
> **240 slots**, carrying five edition-level spreads. `notes.md` called it "235
> slots". The 50-slot unpaired figure survives because no spread falls inside an
> unpaired region.

## What §4 asks for, and what it leaves out

A reader setting that keeps donation pleas, credits cards and recruitment ads
out of the reading flow and puts them in a section that can be opened
deliberately. Not deleted and not hidden: a scanlation group's credit is what it
asked for in exchange for the work, and a reader that silently discarded it
would be taking a side.

§4 named four things as unsettled. All four are settled here:

| Question | Settled as |
| -------- | ---------- |
| Sidecar or client-side? | A per-edition list in `taiyaku.json`, the shape `spreads` has |
| `align`'s job or a separate command's? | A separate command that proposes and writes only under `--write` |
| What the toggle does to page numbering | Nothing — skipping is presentational |
| The false-positive rate | Measured by a survey before any threshold ships |

Out of scope, deliberately: **a reader cannot tag or untag a page.** See
"Correction is not in this feature", below.

## The evidence, measured

Reading `corpus/gotoubun-v01`'s `raw-mono → en-fan` alignment and asking which
of the fan edition's 240 slots no run mentions gives 50 slots. **Split at member
boundaries** — which the first draft did not do, and which is what makes the
geometry mean anything — they are six groups. Page indices are zero-based:

| Pages | n | Member | Local | At member | What it is |
| ----- | -: | ------ | ----- | --------- | ---------- |
| 1–42 | 42 | `c000` | 1–42 | neither end | the prototype one-shot; the raw volume has no such chapter |
| 95 | 1 | `c001` | 51 of 52 | **end** | **donation plea** |
| 168 | 1 | `c003` | 31 of 32 | **end** | **donation plea** |
| 198 | 1 | `c004` | 29 of 30 | **end** | **donation plea** |
| 225 | 1 | `c005` | 26 of 27 | **end** | **staff credits** — TL / TLC / PR / ED names |
| 226–229 | 4 | `c005.5` | 0–3 | start | the volume's own front matter |

Every page in the table below the first row was extracted and viewed. The three
pleas are the same `WE NEED DONATIONS! … lhtranslation.com` card, verbatim, at
175,385, 174,117 and 170,889 bytes with three different SHA-256s — each chapter
re-encoded it, so exact-hash deduplication finds none of them.

**The rule is four for four, and it is the *final*-index half of the rule that
carries it.** Every single unpaired page at a member's last index is a credit
page. Nothing at a member's *first* index is: `c005.5`'s four are the tankōbon
cover, the volume title page, the table of contents and a decorative half-title,
unpaired only because `c000` carries a second copy of the same front matter and
the 1:1 rule matched that copy first. That is a mechanical reason, not a
semantic one, and it is why a first-index rule would be wrong rather than
merely unproven.

**The 42-page group is excluded twice over**, which is worth noting because it
means the length bound is not load-bearing on this volume: it sits at neither
end of its member *and* it is 42 pages long.

**What the rule cannot see is the problem.** `c000` local 43 — the edition's page
43 — is a credits page for a **different group**, `PRESENTED BY: underdogscans +
BOSEN SCAN` over five staff names, where the pleas are `lhtranslation`. One fan
edition mixes releases from at least two groups. The solver **paired** that page,
so it never enters the unpaired set and no amount of shape analysis reaches it.
Precision on this volume is 4/4; recall is at best 4/5, and only 12 of 235 pages
have been looked at, so the denominator is unknown.

**A third group's habits are on disk.** `corpus/kaguya-v01` holds an `en-fan`
edition of 12 chapter files from a different release, listed 2026-08-05.
Nineteen chapter files across three groups is not the population, and this
document does not claim otherwise, but it is what turns one witness into
several.

## The two signals, and why only one of them classifies

**Signal 1 — unpaired, and where it sits. This is the classifier.** Which slots
of the edition no run mentions; those slots grouped into runs; **each run split
at member boundaries**; and for each resulting run, which member it falls in,
its local index range, and whether it ends at that member's last index. Pure
sidecar arithmetic. Needs an alignment.

The split is not a detail. Unsplit, the `c005`/`c005.5` group is one run of five
whose first page is mid-member and whose last page is mid-member, so a rule
reading flags off its endpoints sees neither boundary and misses a true positive
and four true negatives at once.

**Signal 2 — recurrence across sibling members. This is a diagnostic, not a
classifier.** A page's Hamming distance to its nearest neighbour among pages of a
*different member of the same edition*, by dhash-16. Measured across all 235
pages using the fingerprint cache's own values:

| Distance | Pages | What they are |
| -------- | ----: | ------------- |
| 2 | 2 | the volume **cover**, carried by both `c000` and `c005.5` |
| 14 | 2 | two donation pleas |
| 24 | 1 | the third donation plea |
| 33–64 | 2 | ordinary pages |
| >64 | 228 | ordinary pages |

**Three findings, each of which disqualifies it from deciding anything.** Its
tightest matches by a wide margin are a false positive — duplicated front matter
is as common as duplicated credits, and this signal cannot tell them apart
because recurrence is all it measures. Its true positives sit at 14–24 rather
than near zero, because a near-black card has few strong gradients and
re-encoding flips the weak ones; this is the mirror of the blank-page hazard
`status.md` already records, and it leaves little room between signal and noise.
And it misses both staff-credit pages at distances 93 and 99, because each appears once.

**What it is good for is explaining a candidate.** A person reviewing the
command's output wants to know whether a nominated page recurs and where, and
that is a fact worth printing beside every candidate. It is reported, never
decisive.

## Where the code goes

Six pieces. Every dependency edge already exists.

### 1. `taiyaku-core::credits` — signal 1, pure

```rust
pub struct UnpairedRun {
    pub slots: Range<usize>,   // in the edition's flat slot sequence
    pub pages: Range<usize>,   // in the edition's flat page sequence
    pub member: usize,         // index into the edition's members
    pub local: Range<usize>,   // local page indices within that member
    pub at_member_end: bool,   // local.end == that member's page count
}

pub fn unpaired_runs(sidecar: &Sidecar, edition: &str, against: &str)
    -> Result<Vec<UnpairedRun>, CreditsError>;
```

**Every run is split at member boundaries**, so `member` is a single index rather
than a range and `at_member_end` is unambiguous. This is what the measurement
showed the first draft got wrong.

**There is no `at_member_start`.** The first draft had one, to reach `c005.5`'s
four leading pages; those pages turned out to be the volume's front matter, so
the flag exists only to create false positives. Its absence is a decision, and
it is recorded in "Only the final index counts" below so that nobody adds it back
as an oversight.

`pages` sits beside `slots` because the `credits` field is in pages and the
reader addresses slots, and deriving one from the other at each call site is how
that conversion goes wrong in one place and not another.

No threshold lives here. This reports geometry; the command decides. It belongs
in `taiyaku-core` for the reason `route` and `slot` do — it touches no I/O, and
`taiyaku-core` is the portable half.

### 2. `taiyaku-core::sidecar` — the `credits` field

A per-edition array of page indices in the edition's flat page sequence,
identical in shape to `spreads` — zero-based, and pages rather than slots:

```json
"credits": [0, 1, 94]
```

Illustrative, not measured. Parsed, emitted, round-tripped; absent and empty
both mean "nothing classified". `sidecar.md` gains a paragraph beside the
`spreads` one, saying that these are **page** indices like `spreads` and not
slots, and that unlike `spreads` they are a heuristic's output rather than a
measurement of the image.

**One trap here, not the two the first draft claimed.** `notes.md`'s table is
zero-based page indices and transcribes into this field directly — that was
checked by reproducing all five of its rows from the shipped sidecar. The draft
guessed it was one-based because its lowest value is 1; the real reason is that
page 0 is *paired*.

The trap that is real is page against slot. `notes.md`'s prose says "the fan
edition's 235 slots" and the edition has 240 — 235 pages plus five edition-level
spreads. The unpaired figure of 50 is the same either way here, purely because no
spread falls inside an unpaired region, so this volume **cannot** catch a
page-for-slot substitution. Risk 4 below carries that.

### 3. `taiyaku-cli/src/credits.rs` — signal 2

Fingerprints every page of the edition and, for each page, finds its nearest
neighbour among pages of a different member. Reuses `fingerprint.rs` and the
content-addressed cache, so on already-fingerprinted material it costs the
cache's 189 ms rather than a 3.4 s cold pass.

### 4. `taiyaku-cli/src/main.rs::credits` — the command

```
taiyaku credits <member>...   [--sidecar FILE [--against EDITION]] [--cache DIR] [--write]
```

Without `--sidecar` it runs signal 2 alone, which is the single-edition case.
With it, both. It prints a row per candidate page naming which signals fired and
with what values, and one fixed-field `credits:` line for a survey to collect —
the discipline `--stats` established and `duplicates`' `set:` line follows.

**It writes nothing without `--write`**, matching `inventory` and `duplicates`,
which print what they found and leave the acting to a person. `--write`
**replaces** that edition's `credits` list wholesale rather than merging into
it, so a re-run after a threshold changes leaves no stale entries behind. It is
safe to be blunt about this precisely because the field holds no hand entries —
see "Correction is not in this feature", below.

Four refusals, each naming what was missing:

1. A member set matching no edition in the sidecar, printing the hashes it
   looked for.
2. The named edition is the hub and no `--against` was given, printing the
   spokes available.
3. `--write` with no `--sidecar` — there is nothing to write into.
4. `--sidecar` naming a file that will not parse, through the existing sidecar
   error path.

`scripts/credit-survey.sh` is the survey, alongside `margin-survey.sh` and
`duplicate-survey.sh`.

### 5. `taiyaku-wasm` and `web/src/resolver.ts` — reaching the reader

`api.ts` states the discipline: "The sidecar, verbatim, as text — `wasmResolver`
parses it, not `JSON.parse`", so that there is one thing that knows how to read
a sidecar and not two that have to be kept in step. The list therefore reaches
the reader as a new method on the `Resolver` interface —
`credits(edition: string): number[]` — implemented by the binding over
`taiyaku-core::sidecar`, with `fixtures.ts` gaining it for the tests that do not
load wasm.

**`taiyaku-serve` is untouched.** It serves the sidecar verbatim, so a new field
arrives at the client with no server change at all.

### 6. `web/src/modules/credits.ts` — the seventh module

A `ReaderModule` against the existing `module.ts` contract, like the other six.
Two settings, so `Settings.tsx` generates its section from `defaults` with no
special-casing: skipping on or off, and the key that opens the panel.

Skipping steps the arrow over a listed slot. The panel lists every credit page
of the edition — member, slot, and a thumbnail from the tile route the filmstrip
already uses — and picking one goes there. It mirrors the notes panel, including
its habit of listing what is not reachable from where the reader stands rather
than hiding it.

## Decisions

### The list lives in the sidecar, not client-side and not beside it

Three places were considered: `taiyaku.json` as a per-edition list, a
`<stem>.credits.json` beside it in the manner of `<stem>.notes.json`, and
`localStorage` in the manner of the `regroup` module's parity breaks.

The sidecar wins because a credit page is a derived fact about an edition's page
sequence, which is what `spreads` is and what the document exists to hold, and
because the classification should be the same on every device that reads the
work. Parity breaks are genuinely a per-reader preference; this is not.

The cost is that the field has to survive a merge, which is where `spreads` has
never had to be interesting. See the next decision.

### Correction is not in this feature, which is what keeps the field simple

A reader cannot tag or untag a page. The command proposes, prints, and writes
only when told to; a person reviews the output.

This is the decision that keeps the field a plain array of indices exactly like
`spreads` — with no `by: "tool" | "hand"` provenance, no rule for preserving
hand entries across a re-run, and no second instance of the question O24 has
open against `merge_spoke`. Adding correction later adds all three at once, and
adding it now would roughly double the branch.

It is not a silent failure mode. A wrongly-classified page is still listed in
the panel and still one keypress from being read, because the feature's whole
premise is that these pages are set aside rather than hidden. A reader who
disagrees with the classification meets the page in the section rather than
losing it.

It also declines to build a correction UI before the survey has measured how
often the classification is wrong.

### `align --write` must preserve a `credits` list it did not write

`credits` is written by its own command; `align --write` merges editions. So a
merge can land on a sidecar that already carries a list, and the list must not
be dropped by a command that knows nothing about it.

Where a merge leaves an edition's members unchanged — the ordered `sha256`
sequence is identical, the comparison `merge_spoke`'s own guard already makes —
the edition's `credits` list carries across untouched. Where a merge **replaces**
an edition's members, the list addresses pages that may no longer exist, and the
merge drops it, reporting how many entries it dropped.

Dropping rather than refusing is the opposite of O13's stance on hand-corrected
runs, and deliberately so: O13 refuses because a hand correction is a human
decision that cannot be recovered by re-running anything. A `credits` list is a
tool's output, recoverable by running `taiyaku credits` again. That difference is
exactly what the previous decision bought by keeping hand entries out of the
field.

### Signal 1 needs a counterpart named, and refuses to guess one

"Unpaired" is unpaired *against something*. Where the named edition is a spoke,
the counterpart is the hub it stores an edge to, which is the real case:
scanlation credits live in fan editions and fan editions are spokes.

Where the named edition is the hub, there is no single edge, and the command
refuses until `--against` names a spoke rather than silently picking the first.
The refusal names the spokes available.

### The edition is identified by content, not by name

The command is handed members, not an edition key. With `--sidecar` it matches
them to an edition by ordered `sha256` sequence — the idiom `merge_spoke`'s
guard already states: "Content, not paths… only the hashes can be compared."

A member set matching no edition is refused by name, printing the hashes it
looked for. This also means a renamed-but-identical archive is still recognised,
which comparing paths would not manage.

### Skipping changes what is drawn, never what a slot is called

The slot is the addressed unit and `sidecar.md` is normative about it. A
per-reader toggle that renumbered would mean a note anchored to slot 96
addressing a different page for each reader, and progress written on a phone
with skipping on being misread by a desktop with it off — the failure the slot
model exists to prevent.

So the readout still says "slot 96 of 235" and visibly jumps over a skipped
slot. This is the swap precedent exactly: a swap changes what is shown, never
what is written.

### Only the final index counts

A candidate is an unpaired run, split at member boundaries, ending at its
member's last page. Not its first page.

On the one volume measured, that rule is four for four, and the first-index
version would be zero for four. The asymmetry is not an accident of one group's
habits: a credit page is something appended after the chapter, whereas the
things that accumulate at a member's *start* are front matter and title pages,
which are exactly what a reader opened the book for. A group that does put its
credits first would defeat this, and finding one is what would reopen the
decision.

`at_member_start` is therefore absent from `UnpairedRun` by intent. Adding it
requires evidence, not symmetry.

### One threshold comes from a survey, and may not exist

The command reports both signals per page. Which pages it writes is fixed by
`scripts/credit-survey.sh` against the corpus, in the manner of the 27.0
average-run-length threshold, which came from a corpus survey and not from an
argument.

**One number, not two:** the maximum length of an unpaired run at a member's
final index that counts as credits. The dhash distance is no longer a threshold
because signal 2 no longer decides anything — it is printed beside each
candidate as a diagnostic.

The measured material puts that number somewhere above 1 and below 42, and is
not more precise than that: every true positive here is a run of exactly 1, and
the only long run is excluded by position as well as length. **So the corpus
does not currently constrain the bound at all**, which is a finding rather than a
gap to paper over. The survey's job on the Kaguya edition is largely to find a
run of 2 or 3 and see what it is.

**Every candidate is extracted and looked at**, which is how all three of the
first draft's errors were found. A number that separates cleanly and classifies
the wrong pages is the failure this project keeps finding by hand, and only an
eye catches it.

**If no clean separation exists, no threshold ships.** The command still prints
both signals and writes nothing automatically; `--write` then takes an explicit
list of page indices, and the reader module still works against a hand-assembled
list. That is roadmap item 1's outcome exactly — margin refusal was measured, did
not fit in a one-bit gap, and did not ship — and it is a branch of this design
rather than an admission of failure.

### Paired credit pages are out of reach, and the design says so

`c000` local 43 is a credits page the solver paired, so signal 1 cannot see it
and signal 2 cannot either — it appears once. Nothing in this design finds it.

This is stated rather than solved because the fix is a different kind of thing:
it would mean classifying a page on its own content — text density, a solid
background, a URL — rather than on its relationship to other pages, and that is a
classifier with its own corpus and its own false-positive problem. The honest
scope here is "the credit pages the alignment already exposes", and a reader who
meets a missed one meets it in the reading flow exactly as they do today.

### The command is wired into the CLI's existing argument rules

O19, O20, O21 and O22 are four separate open-and-closed items with one cause: a
surface that did not apply a rule the rest of the CLI already had. `credits`
adds three valued flags, so it is exactly where that recurs.

`--sidecar`, `--cache` and `--against` go through `option_value`, so a flag typed
with no usable value is refused rather than taken as absent. The command is
listed in `reject_unknown_flags`, so a misspelling is refused rather than
ignored. Every valued flag resolves before any member is read, so a typo does
not cost the seconds it takes to fingerprint an edition.

## Testing

### Unit tests in core, on fixtures

The geometry cases that decide the rule:

- a run at a member's final index — the donation-plea shape, classified
- a run at a member's first index — the `c005.5` front-matter shape, **not**
  classified, which is the assertion that keeps `at_member_start` from coming
  back
- a run mid-member, touching neither end
- **a run spanning a member boundary splits into two** — the `c005` local 26 plus
  `c005.5` local 0–3 shape, asserting one run at an end and one at a start, which
  is the case the first draft got wrong in both directions
- **a spread inside an unpaired run**, so page indices and slot indices disagree
  by construction and the `pages`/`slots` fields cannot be transposed silently.
  Nothing in the corpus exercises this — see risk 4
- an edition with nothing unpaired
- the hub case, refused for want of `--against`

Plus a `credits` round-trip: present, empty, absent. Plus a merge test per half
of the preservation rule — unchanged members carry the list, replaced members
drop it and report the count.

### Two gated corpus checks, added to `check-all.sh`

Gated on `TAIYAKU_CORPUS`, skipping cleanly when it is absent, and **added to
`check-all.sh`** — a gated check the script does not run is invisible to the
gate, which is the point of that script.

1. **Go-Toubun v01 `en-fan`** reproduces the measurement recorded above: exactly
   four classified pages — 95, 168, 198 and 225 — with `c000`'s 42-page run and
   `c005.5`'s four front-matter pages both unclassified. Asserting the *set*
   rather than the count matters, because 4-of-6 is reachable by classifying the
   wrong four. Prints what it measured under `--nocapture`.
2. **Kaguya v01 `en-fan`** runs the third group, 12 chapter files. This one has
   no recorded expectation to reproduce; it establishes one, and it is where a
   run of length 2 or 3 would first appear.

### In `web/`, the contract and the numbering

The module meets the `module.ts` contract the other six do — registered,
defaults generate a `Settings.tsx` section, switching it off removes the
behaviour entirely.

And the test that carries the numbering decision: **skipping on and skipping off
produce identical slot numbers and identical progress writes.** That is the
whole content of "presentational", and it is the test that goes red if someone
later renumbers for readability.

### Not tested

That a person agrees the classified pages are credits. The survey establishes
that by extraction and eye, and its output belongs in `notes.md` as a
measurement rather than in the suite.

## Risks, named

1. **Three groups is not the population, and one volume is the whole basis.**
   Every number in this document comes from Go-Toubun v01's fan edition, of which
   12 pages out of 235 have been looked at. Two groups appear in that one edition
   and a third is in Kaguya. The design's answer is that the bound is a
   measurement with a stated sample size, not a constant, and that the survey
   script is kept so a fourth group can be run through it.
2. **The rule is one volume from being wrong.** A group that puts its credits at
   a member's first index defeats it entirely, and a group whose credits run to
   two or three pages is unconstrained by anything measured. Neither has been
   seen; neither has been ruled out. The design's answer is that the rule is
   stated in terms of what it was measured on, and that Kaguya is run before the
   bound is fixed.
3. **The recall gap is real and unbounded.** `c000` local 43 is a credits page
   the solver paired, and nothing here finds it. How many more of the 223
   unexamined pages are in that category is unknown. This is the risk most likely
   to make a reader say the feature does not work, because it fails silently and
   in the reading flow.
4. **A colour insert recurring across members would look exactly like a credits
   card to signal 2.** The volume cover already does, at distance 2. This is why
   signal 2 does not classify; the risk is retired by that decision rather than
   managed.
5. **`spreads` and `credits` index pages; the reader addresses slots; and the
   one volume with a recorded measurement cannot tell the two apart.**
   Go-Toubun's `en-fan` has five spreads, all of them outside every unpaired
   region, so page indices and slot indices give the same answer for all 50
   unpaired positions. A page-for-slot substitution passes this volume silently.
   That is the shape of all four M5c browser defects — invisible to a check
   comparing this code's output to its own other output. **The mitigation is a
   fixture, not a corpus check:** a unit test whose edition has a spread *inside*
   an unpaired run, where the two indexings disagree by construction.

## Shape of the work

Roughly in order, each a landable step:

1. `taiyaku-core`: the `credits` field, its round-trip, and the merge
   preservation rule.
2. `taiyaku-core::credits`: `unpaired_runs`, boundary splitting, and its fixture
   tests — including the spread-inside-a-run fixture risk 5 names.
3. `taiyaku-cli/src/credits.rs`: the dhash recurrence pass, reported only.
4. `main.rs::credits`: the command, its refusals, and the argument-rule wiring.
5. `scripts/credit-survey.sh` and the survey run over both fan editions — **the
   bound is chosen here and the two corpus checks are written against what it
   measures**, not before.
6. `taiyaku-wasm` and `resolver.ts`: `credits(edition)`.
7. `web/src/modules/credits.ts`: skipping, the panel, and the numbering test.

Steps 1–4 do not depend on the survey. Step 5 is the gate between proposing and
writing, and steps 6–7 work against a hand-written list if step 5 comes back
without a bound.

**The prototype for steps 2 and 3 has already been run** — both signals, against
the real sidecar and the real fingerprint cache — and its results are the
measurements quoted throughout this document. The scratch scripts are not kept;
the numbers and the six-row table are the artefact.

## What this leaves open

- **Correction from the reader**, deliberately deferred above. Whether it is
  wanted at all is a question the survey's false-positive rate answers.
- **Credit pages the solver paired**, of which `c000` local 43 is one. Reaching
  them means classifying a page on its own content rather than on its relation to
  others — a different feature with its own corpus.
- **Recruitment ads and group logos** are named in §4 and are not separately
  evidenced anywhere. The two credit shapes actually observed are a donation card
  and a staff-name page.
- **An edition whose credits are byte-identical across members** would be found
  by exact hashing more cheaply than by dhash. Not worth a special case until
  something in the corpus is like that.
- **Whether skipping should be per edition or per work.** The list is per
  edition, but a reader who turns skipping on has expressed a preference about
  reading, not about an edition. Nothing forces the question until a work has two
  editions that both carry credit pages.
- **What a third edition of one work does** when two of its editions carry
  different credit pages. Nothing here is cross-edition; the list is per edition
  and travels nowhere through the alignment.

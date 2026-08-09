# L2 text regions — design

**Status: settled in brainstorming on 2026-08-08. Nothing built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here is a measurement
already recorded in `notes.md`, and a guess says it is one.

## What L2 turned out to be, and where the existing note is thin

`docs/status.md` carries L2 in its **Later** list as:

> **L2 text regions** — bubble pairing by transformed centroid, which L1's
> `mapRect` now makes arithmetic rather than a matching problem.

That line is right about the half it describes and silent about the half that is
the work.

**The pairing really is arithmetic now.** `mapRect` (`web/src/viewport.ts:623`)
maps a rect from one edition's frame into another's through `pillarbox`, chosen
by measurement over 284 settled slots of 288 sampled from twelve corpus volumes
across four series — median residual 0.0195 of page width and 0.0041 of page
height, p90 0.0276 and 0.0116. `notes.ts` already carries a hand-drawn
annotation region between editions through that same call plus `halfBox`. So the
machinery for "take a box on the raw and name it on the English page" ships
today, and L2 does not have to invent it.

**The line says nothing about where the boxes come from, and that is the
milestone.** Nothing in this repository detects anything on a page. "Bubble
pairing by transformed centroid" presumes a bubble on each side to pair, and
producing those is the larger half of this work by a wide margin.

**"Bubble" also prejudges what a region is, and the answer is no.** This design
detects **text blocks**, not balloon outlines. The reasons are in §1; the
shortest one is that a balloon detector is blind to borderless narration, which
manga uses constantly, and that the text block is the box a later text-selection
or lookup pass actually wants.

`sidecar.md`'s forward-compatibility section says "L2 adds matched text regions"
to the sidecar. This design puts them in a sibling file instead, for reasons of
size rather than of principle — see §2. Both lines should be corrected when this
lands; they are listed in §13.

## What L2 is for

**Chosen: peek the counterpart balloon.** A reader working through the raw
points at a balloon and sees the *pixels* of the matching English balloon beside
it — a crop of the other edition's artwork — without leaving the page they are
reading, without swapping, and without any text extraction whatsoever.

This is the same claim M5c already made for hand-written notes ("a note written
on the raw is readable while reading the English") extended from a region a
person swept out by hand to every text block on the page, found automatically.
It is a reading experience nothing outside this project can offer, for the same
reason the swap is: it needs a per-slot alignment between two editions of one
work, which is what the sidecar is.

**Not chosen, and worth recording because each was a live option:**

- **Balloon-precise swap and stepping.** Sharpening L1's swap so it lands on the
  exact counterpart region rather than a mapped rect, plus keys to step balloon
  by balloon in reading order. Deferred because L1's residual is already "a
  fraction of a balloon rather than a panel" (p90 2.8% of page width, about
  60px), so the sharpening buys less than the peek does, and it can be built on
  top of the same regions afterwards.
- **Regions as a solver input.** `notes.md` records that removing text costs
  roughly 0.24 of similarity and that the hash is "invariant to text
  **substitution**, not to text **presence**", which makes cleaned and redrawn
  editions the corpus's hardest case; it names masking bubble interiors before
  hashing as the way through. That is a real use for regions and a different
  milestone — it changes the aligner, and this one changes neither the aligner
  nor the solver.
- **Groundwork for OCR.** Regions are where text lives, and a later pass could
  read them. Deliberately not this milestone, but §7 records one decision taken
  now on its behalf, because getting it wrong later would be expensive.

## Scope

**In, and built in this plan:**

- `taiyaku regions`, a ninth command: detect text blocks in one edition, print
  what it found, write only under `--write`.
- A sibling document, `<stem>.regions.json`, holding regions per edition per
  page.
- `taiyaku_core::regions`, the pairing rule, exposed through `taiyaku-wasm`.
- A `regions` `ReaderModule` and the peek it draws.
- One gated corpus check, and the experiment that picks the pairing rule.

**Out, stated so the boundary is not argued later:**

- No OCR, no text extraction, no typesetting, no translation.
- No change to the solver, the aligner, `verdict`, or any alignment.
- No stored pairing (§5), and no `--against`-style cross-edition write:
  `taiyaku regions` detects one edition at a time.
- No balloon-outline detection (§1).
- No new dependency. `image` and `rayon` are what `align` already uses; the
  connected-component labelling is written here rather than pulled in.

## Shape of the work

**This is more than one plan, and saying so here is part of the design.** M5 was
split into M5a–M5d for the same reason, and the seams below are chosen so that
each stage is shippable and measurable on its own rather than being a slice of
one long branch.

| Stage | What lands | Measurable on its own by |
| --- | --- | --- |
| **L2a** | Detection, `taiyaku regions`, the region document, `scripts/region-survey.sh` | The survey: blocks per page, axis split, median area across the corpus |
| **L2b** | `taiyaku_core::regions`, the wasm binding, the §6 experiment, the standing check | The experiment, which either promotes a candidate or promotes the baseline |
| **L2c** | The server route, the `regions` module, the peek | A browser drive against real material |

**L2b is the one that can end the milestone.** If its promotion rule fires
against every candidate, L2c ships `blind-crop` and the detector built in L2a
becomes a thing the corpus knows about rather than a thing the reader uses. That
is an acceptable outcome, it is why the stages are in this order, and it is why
L2a's survey is worth having even then.

## 1. What a region is

**A text block: a cluster of glyph-sized dark connected components, plus the
axis it runs along.** Not a balloon interior, and not a whole panel.

Three candidates were considered and the choice is recorded here because it is
the one that most shapes everything downstream.

| Target | Pairs like-for-like | Catches borderless narration | Serves later lookup | Cost |
| ------ | :---: | :---: | :---: | --- |
| Balloon interiors | yes — one language-agnostic detector | **no** | loosely | lowest |
| **Text blocks** | yes, after a per-language clustering rule | **yes** | **exactly** | one rule per axis |
| Both, nested | yes | yes | yes | two detectors plus a nesting rule |

**Balloon interiors lost on recall.** A white blob fully enclosed by a dark
border is the easiest thing on a manga page to find and it is not where a large
share of the text is: narration, asides and off-panel dialogue are frequently
set with no outline at all, and a detector that cannot see them produces a page
where some balloons peek and some silently do not — which reads as the feature
being broken rather than as a boundary.

**Nested lost on size, not on merit.** It is the better long-term shape and it
self-checks (a text block inside a balloon is dialogue; one outside it is
narration). It is two detectors and a containment rule for a milestone that can
ship with one, and the region document's shape does not foreclose adding
`balloon` as a second kind later.

**Text blocks won on the thing the reader asked for.** A tighter box gives a
tighter centroid, which makes the pairing more accurate rather than less, and it
is the box an OCR or dictionary pass wants first. That last point is a stated
future rather than a measured present, and it is the one place in this document
where a decision is taken for a milestone that is not scheduled.

**Five fields**, in page fractions, which is the unit every other geometry in
this project uses:

```json
{ "x": 0.12, "y": 0.08, "w": 0.11, "h": 0.31, "axis": "v" }
```

`axis` is `"v"` or `"h"`. It is what the clustering already knows, so storing it
costs nothing; it is the first thing a later OCR pass needs; and it is a cheap
correctness check that needs no ground truth — a Japanese edition coming back
mostly `"h"` means the detector is wrong, and the command can say so.

## 2. Where regions live, and why not the sidecar

**A sibling file: `<stem>.regions.json`, in the sidecar's directory, on the
sidecar's stem.**

**The sidecar cannot hold them, on size.** The real Go-Toubun v01 alignment is
five runs — the whole document is a page of JSON — and its first two design
goals are "human-readable, diffable in git". Regions are per page, not per
edition. *Estimated, not measured:* 195 pages at roughly six text blocks a page
is about 1,170 boxes, at roughly 48 bytes of JSON each, so about 56 KB for one
edition and about 280 KB across that document's five. Two orders of magnitude
onto a file a person is expected to read and hand-correct. The estimate is a
guess about blocks per page; the conclusion survives any plausible value of it,
which is why it is not worth measuring first.

**The convention already exists, twice.** `<stem>.notes.json` holds annotations
beside the sidecar, read and written whole under `If-Match`; `taiyaku.stump.json`
holds Stump resolutions in the same directory on the same stem. A third file of
the same shape introduces no new idea, and `taiyaku serve` already knows how to
serve one.

**This is stronger forward compatibility than the sidecar rule, not weaker.** An
L0 reader ignores keys it does not recognise; an L0 reader never fetches this
file at all.

**Keyed by page index in the edition's flat sequence**, the same addressing
`spreads` and `credits` use, so the normative page-sequence rules carry unchanged
and no new addressing has to be specified:

```json
{
  "taiyaku_regions": 1,
  "editions": {
    "raw-mono": {
      "members": [ { "sha256": "3f9a…" } ],
      "pages": {
        "4": [ { "x": 0.12, "y": 0.08, "w": 0.11, "h": 0.31, "axis": "v" },
               { "x": 0.61, "y": 0.44, "w": 0.09, "h": 0.22, "axis": "v" } ],
        "5": []
      }
    }
  }
}
```

**`members` carries the hashes and nothing else**, in the sidecar's own order.
It is what lets a reader tell that an archive was replaced and these boxes now
describe art that is no longer there — the same job `sha256` does in the sidecar,
where the document already says "a reader can tell the archive was replaced and
the alignment is stale". Regions have the sharper version of that problem: a
stale alignment sends a reader to the wrong page, and stale regions draw a box
on the wrong art on the right page, which is harder to notice. On a mismatch the
edition's regions are ignored rather than drawn (§11).

**Page, not slot, deliberately.** A joined spread is one image and detection runs
on the image; the reader already knows how to take a half out of a page, and
`notes.ts` does exactly that with `halfBox` today. Storing halves would mean the
detector deciding where the fold is, which is the spread rule's job and not this
one's.

An absent file and an empty object both mean nothing has been detected, matching
`credits`' rule. A page absent from an edition's map means the same thing; an
explicit `[]` means detection ran and found nothing, which is a different fact
and worth being able to state.

## 3. `taiyaku regions` — the ninth command

```
taiyaku regions <member>... --sidecar FILE [--write]
```

Same contract as `credits`, which is the closest existing command: **it proposes
and prints; it writes only under `--write`.**

*Corrected on 2026-08-08, while planning L2a.* This signature carried a
`[--cache DIR]` copied from `credits`, and the command has no use for one: the
fingerprint cache holds hashes and dimensions rather than pixels. Accepting it
anyway would break the rule O19, O20 and O22 each closed a defect to establish —
a flag that cannot take effect is refused rather than ignored — in the act of
citing it below. See the L2a plan, "Deviations from the spec". It identifies the edition by
content — hashing the members it was handed and matching them against the
document — rather than being told or guessing, which is the rule `verify`
established and the reason a five-edition document cannot be silently addressed
by position.

It prints a table for a person and one fixed-field line for a survey, the
discipline `--stats` established and `duplicates` and `credits` both follow:

```
page  blocks  v     h    median area
   4       6  6     0    0.021
   5       3  3     0    0.034
...
regions: edition=raw-mono pages=195 blocks=1170 v=1131 h=39 median_area=0.024 blocks_per_page=6.0
```

`--write` writes `<stem>.regions.json` through `atomic.rs::write`. O26
(`align --write` truncating the sidecar before writing it) and O29 (a failed
write naming a temp path the caller never typed) are both already paid for
there, and this is the third writer to benefit rather than the first to
rediscover them.

Unknown flags are refused, per O21; flags that cannot take effect are refused
rather than ignored, per O19, O20 and O22.

## 4. Detection

Four stages, all in `taiyaku-cli`, no new dependency:

1. **Decode to luma.** `taiyaku-read` hands over the page bytes and `image`
   decodes them, subject to the `MAX_DECODE_ALLOC` ceiling that moved to
   `taiyaku-read` when O10 closed.
2. **Threshold.** Manga is very nearly bilevel already, which is why this is
   tractable at all without a model. The threshold rule is an implementation
   choice for the plan, not settled here; whatever it is, it is one function
   with its own unit tests.
3. **Label connected components** of dark pixels, and keep the glyph-sized ones.
   "Glyph-sized" is a bound on component area relative to page area, and it is
   **measured, not chosen** — surveyed across the corpus the way `MAX_CREDIT_RUN`
   was, and reported by `scripts/region-survey.sh`.
4. **Cluster components into blocks, per axis.** Japanese runs vertical and
   English typeset runs horizontal, so this is two rules rather than one: group
   components whose spacing along the axis is small relative to their own size,
   then take the bounding box. The axis is decided per cluster and reported, so
   an edition that comes back on the wrong axis is visible rather than silent.

   *Corrected on 2026-08-08: **L2a decides the axis once per page, not per
   cluster**, and shipped that way without recording it as a deviation — which
   is what this correction is for.* `detect_page` calls `axis_of` over the whole
   page's components, uses that one answer for the single `merge_along`, and
   stamps it on every region. Three consequences, none of them fixed in L2a:
   a page carrying vertical dialogue beside a horizontal SFX or narration line
   cannot report both, which is the case this stage's last sentence exists for;
   the smear itself runs on that one axis, so a page whose global axis resolves
   wrong is smeared wrong end to end, which is a second plausible contributor to
   the measured failure that `notes.md`'s diagnosis does not name; and **the
   survey's `v=`/`h=` figures are block counts of a per-page decision**, so the
   effective sample behind "5 of 9 nominally correct" is pages rather than the
   148–402 blocks the table shows. The evidence is coarser than it reads.
   The justification for storing `axis` on a region — "the clustering already
   knows it", in §1 and in `taiyaku_core::Axis`'s own doc — is therefore not yet
   true of the code.

Stages 3 and 4 parallelise per page with `rayon`, as fingerprinting already
does, through the same bounded decode `decode_all` performs — the corpus holds
430 MB TIFFs, and decoding the large pages one at a time is what keeps a run
inside memory.

*Corrected on 2026-08-08, while planning L2a.* This paragraph said results were
cached under `.taiyaku-cache/` keyed on the member's SHA-256, and that a third
cache in that tree was worth naming in the O11 entry. **No third cache ships.**
It would hold the same boxes the regions document already holds, keyed on the
same hash, in a second place that can disagree with the first — the duplication
O10 was closed to remove. `--write` is what makes a second run cheap. What goes
in the O11 entry is therefore the opposite of what this said: that a feature
which looked like the second consumer that entry names is not one. If a
full-corpus run turns out slow enough to matter, the survey says so and a cache
can be added then, against a number.

**What detection is not asked to do:** separate SFX from dialogue, read
anything, or decide whether a block is speech or narration. A 効果音 is a text
block, it has no English counterpart region, and it pairs with nothing — which
is the correct answer and is reported as unpaired rather than guessed at.

## 5. The pairing rule, and where it lives

**In `taiyaku_core::regions`, exposed through `taiyaku-wasm`. Not in TypeScript
beside `mapRect`.**

This placement is the direct fix for a trap `docs/status.md` §4 already
documents. The L1 geometry experiment could not be promoted to a standing check
because the transform it picked is TypeScript, so "a Rust corpus check would
re-measure page dimensions no code in this repository can alter and go green
while the browser did something else." Putting the rule in the portable core
means one corpus check measures exactly what the reader runs.

**It costs nothing on any hot path, because there is not one.** `mapView` — the
only caller of `mapRect` outside tests — has two production call sites:
`App.tsx:1625`, the swap, and `notes.ts`, for travelled note regions. Zoom and
pan go through `zoomAt`, `panBy` and `clampRect` and never map between editions.
A WASM call per peek is comparable to a WASM call per swap, which already ships.

The rule, given a page of the viewed edition:

1. **Resolve the counterpart slot through `route`** — the same call the swap and
   the notes make, which means a composed answer is composed here too and says
   so. No counterpart resolves to: every region on that page is unpaired, drawn
   in the gap vocabulary that already exists.
2. **Map each region's centroid** through `pillarbox` into the counterpart
   page's fractions, via `halfBox` where either side is a joined spread.
3. **Choose the far-side region: containment first, nearest centre within a
   bound as fallback.** The bound is measured, not chosen (§6).
4. **Enforce 1:1.** Where two regions claim one counterpart, the smaller
   distance wins and the loser is unpaired. This is the alignment's own rule,
   and O23 stands as the reminder of what 1:1 costs when it bites.
5. **Report unpaired as unpaired.** Never a guess, never a nearest-anything with
   no bound. The same conservative direction the composition loss takes, where a
   reader sees "no counterpart" rather than a wrong page.

**The pairing is derived at read time and not stored.** Three reasons, and the
third is the one that decides it:

- **N region lists, not N² pairings.** A fourth edition costs one detection run,
  not a re-pairing of everything against everything.
- **A hand correction to the alignment re-pairs for free**, because the pairing
  is a function of the alignment rather than a snapshot of it.
- **A stored pairing is a derived thing that outlives what it derived from**,
  which is O24's open failure mode exactly: a solver-written edge surviving the
  replacement of an edition it addresses, and then being reported at the
  strongest confidence the format has. Writing pairings into a file would import
  that failure into a second place, in a milestone that is not fixing it in the
  first.

Storing them later stays permitted at no cost by `sidecar.md`'s ignore-unknown-keys
rule, which is the same ground L1 deferred its stored transform on and the same
ground the gatefold question is deferred on.

## 6. The experiment that picks the pairing rule

**Candidates and the promotion rule are fixed here, before any number exists**,
so that the result decides the cost rather than the cost deciding how the result
reads. This is the discipline `geometry_experiment.rs` used, where the rule's
first branch fired and the baseline beat one of the two named candidates.

| Candidate | Far-side region chosen by |
| --- | --- |
| `contain` | the mapped centroid must fall inside one |
| `nearest` | nearest centre within a bound |
| `contain-then-nearest` | containment, falling back to nearest within a bound |
| **`blind-crop`** (baseline) | **nothing — no far-side detection at all** |

**`blind-crop` is the baseline deliberately.** It is exactly what ships if there
is no detector on the far side: take the source region's whole box, map it
through `pillarbox` — `mapRect` of the region rather than of its centroid, which
is the call that already exists — and crop that. Sizing it from the source
rather than fixing a box size is what makes it a fair baseline rather than a
straw one: it already uses everything L1 knows.

Its failure is a crop that clips the balloon or includes its neighbour, bounded
by L1's residual; a candidate's failure is showing nothing. The detector has to
earn its place against the thing that needs no detector.

**`blind-crop` is scored on the same metrics with one substitution.** It has no
far-side regions, so mutual agreement is undefined for it; it is scored instead
on how often its mapped box would have *contained* the far-side region the
winning candidate paired to, which is the same question asked of the only
artefact it produces. Where the two disagree, the disagreement is the finding
and gets written up rather than averaged away.

### Measured without labels

The corpus has no labelled balloons, and hand-labelling a sample would measure
the labelling. The measurement is therefore **cross-edition agreement**: detect
both editions independently, pair A→B and B→A, and report how often the two
directions choose each other.

A degenerate detector cannot fake this. If one side emits a few page-sized
boxes, 1:1 enforcement collapses the rate — six raw regions claiming one
English region means five unpaired — so the metric defends itself against the
failure that would otherwise game it.

Reported for every candidate over the same sample:

- **mutual agreement** — the headline figure.
- **pair rate** — the fraction of regions that get any 1:1 counterpart.
- **median region area**, as a fraction of the page.
- **median regions per page**, on each side.

### Two absolute disqualifiers, stated now

- **median region area > 0.15 of the page** — the boxes are not text blocks.

  *Corrected on 2026-08-08, after L2a's whole-branch review: **this disqualifier
  cannot fire and never could.*** `MAX_BLOCK_AREA` in `detect.rs` discards any
  block above 0.15 of the page *before* a `Region` is constructed, and
  `median_area` is computed over the regions that survive — so every emitted
  region satisfies the bound by construction and the median is clamped under it
  whatever the detector does. L2a's survey duly reported it passing on all nine
  editions while `blocks_per_page` failed on all nine, which is how a check that
  measures nothing reads when everything is broken.
  **A block-size check has to be taken before the filter, not after it**, over
  the merged blocks as they come out of `merge_along`. Until it is, this leaves
  L2b's promotion rule below with one live disqualifier rather than two, and the
  rule's wording — "if no candidate clears **both**" — must not be read as
  though two were doing work.
- **mean regions per page < 2** — under-detection. A manga page with fewer
  than two text blocks is a splash page, not the norm.

  *Corrected on 2026-08-08: this said **median**, and both the command and the
  survey compute a **mean**.* Named rather than quietly reconciled, because on
  the detector as built the two differ sharply and the mean is the weaker claim:
  most pages return zero blocks, so the median is 0 on nearly every edition
  while the mean reaches 0.84. Failing a bar of 2 on all nine editions *on the
  mean* is therefore the conservative reading, and the median would fail harder.
  Unlike the disqualifier above it, this one does fire.

Either one disqualifies a candidate regardless of how well it scores on
agreement.

**If no candidate clears both disqualifiers, no detector ships**, L2 falls back
to `blind-crop`, and that outcome is written up in `notes.md` rather than
retried until it passes. This sentence is the promotion rule's whole point and
is the reason it is written before the experiment rather than after.

**Sample:** the same twelve corpus volumes across four series that §5 of
`status.md` sampled, so the two measurements are comparable and the second can
be read against the first.

**Unlike the L1 experiment, this one becomes a standing corpus check**, because
the rule it picks is Rust in the portable core and is what the browser runs.
That is the entire payoff of the placement decision in §5, and it is why the
same experiment produces a different artefact this time.

## 7. The peek

**A crop of the counterpart page, in a floating box that never reflows the page
underneath** — the chrome rule the filmstrip and readouts already obey.

**It costs no new route and no new bytes.** The crop is an `<img>` of the
counterpart *page*, at the same `/pages/` URL the swap uses, inside a clipping
box positioned and scaled so the paired region fills it. Prefetch has often
fetched it already, and since O1 closed — `PAGE_CACHE_CONTROL`, `private,
max-age=86400`, measured at zero page requests across a ten-slot re-read — a
repeat costs no round trip at all rather than a cheaper one. That close is what
makes hovering balloon after balloon affordable; before it, every peek was a
conditional request the reader waited on.

**One decision taken now for a milestone that is not scheduled.** The crop is a
real positioned element containing a real `<img>`, not a CSS `background-image`
and not a canvas, and it does not capture pointer events over its interior. A
later text-selection or lookup pass adds a transparent text layer *inside* that
container — the shape every PDF viewer uses. Building the peek as a background
or a canvas would foreclose it, and the cost of not foreclosing it is zero
today.

**Unpaired regions say so.** `healthLine`'s `no counterpart here` string already
exists and already had no caller until `gapHealth` reached it; this is its
second.

## 8. The module

A `regions` `ReaderModule` against the existing `module.ts` contract — the tenth
claiming reading behaviour, of thirteen registry entries. Its preferences
generate their own settings section from `defaults` with no hand-wiring, a
value's runtime type deciding its control; its keys go in `keys.ts`'s one
ordered table, so `?`/`h` describes them from the same table `intentOf`
dispatches from and there is no second description to drift.

| Preference | Default | Why |
| --- | --- | --- |
| `peek` | on | It is the feature. |
| `marks` | **off** | Outlining every detected region draws on the art. `annotations` shipped `reveal` off for exactly this reason — the margin tabs already said where the notes were. |
| `trigger` | `hover` | Touch has no hover, so `tap` is the other value rather than a separate mechanism. |
| `unpaired` | say so | The alternative is silence, and silence is what this project is written against. |

Keys are checked against the live registry at startup, as `zoom`'s three are.

## 9. Where this touches shipped code

- **`taiyaku-core`** — a new `regions` module. No change to `solve`, `slot`,
  `sidecar` or `route` beyond calling `route`.
- **`taiyaku-wasm`** — one new binding, alongside parse/resolve/path.
- **`taiyaku-cli`** — a new detection module and a ninth command in `main.rs`.
- **`taiyaku-serve`** — one route pair for `<stem>.regions.json`, modelled on
  the notes routes, carrying **no freshness** on either the `200` or the `304`:
  it is a document this server can rewrite, and O1's own regression was putting
  a page's freshness on a mutable document's `304`.
- **`web/`** — a new module, the peek component, and a `Resolver` addition.
- **`taiyaku-read`** — untouched.

Nothing in the aligner, the solver or the alignment changes, which is the scope
decision rather than an accident.

## 10. Testing

1. **Detection**, unit-tested on synthetic bitmaps with known blocks — cheap,
   and it pins each axis's clustering rule independently.
2. **`taiyaku_core::regions` pairing**, unit-tested: 1:1 contention, the
   spread-half case, the no-counterpart case, and the involution — pairing A→B
   and back returns the region it started from, the property `mapRect` already
   holds and the one a wrong formula silently breaks.
3. **One gated corpus check** under `corpus::`, so `check-all.sh` picks it up
   with `--nocapture` and it becomes the thirteenth standing check. **Asserted
   on sets and properties, not counts** — the credits lesson, where four of six
   is reachable by classifying the wrong four.
4. **A browser drive against real material**, which is where this project's
   reader defects actually surface — every one of M5c's four came from a browser
   and none from a unit test. Served from a scratch sidecar with absolute member
   paths, so nothing written can reach the gitignored corpus.

The experiment of §6 is a measurement rather than a check while it is running,
and the check in item 3 is what it leaves behind.

## 11. Failure modes worth naming

- **A page whose detector finds nothing** peeks nothing, and says nothing is
  detected rather than silently behaving as though the page has no text.
- **SFX pairs with nothing**, correctly, and shows as unpaired. A reader who
  points at one gets "no counterpart here", which is true.
- **A gap** — no counterpart slot — makes every region on the page unpaired.
  The vocabulary for this exists.
- **A `FRAGMENTED` alignment** pairs regions through a bad alignment and the
  peek will be wrong. It is wrong for the same reason the swap is, `route`
  already reports the weaker leg of a composed answer, and nothing here refuses
  on a verdict that `lookup` itself only reports.
- **A stale `<stem>.regions.json`** describes an edition whose archive was
  replaced. The member `sha256` is recorded per edition in the region document
  for exactly this, and a mismatch means the regions are ignored rather than
  drawn on the wrong art.
- **A cleaned or redrawn edition** has had its text removed and will detect
  almost nothing. That is correct behaviour on material `notes.md` already calls
  the corpus's hardest case.

## 12. What this deliberately does not answer

- Whether a stored pairing is ever worth it. Deferred on the same ground L1
  deferred its stored transform, and reopened by the derived one proving
  insufficient.
- Whether balloon outlines are worth detecting as a second kind. The document
  shape permits it; nothing here needs it.
- Anything about reading the text. No OCR, no lookup, no dictionary — only the
  one shape decision in §7 that keeps them cheap later.
- Whether regions should feed the solver for cleaned editions. A different
  milestone, and it changes the aligner.

## 13. Documentation that goes stale when this lands

- `docs/sidecar.md`, forward compatibility: "L2 adds matched text regions" —
  corrected, not deleted, the way the L1 line above it was corrected when L1
  shipped. Regions are a sibling document; the pairing is not stored at all.
- `docs/status.md`, **Later**: the L2 line leaves the list. Kept and corrected in
  place, as L1's was, because a list of future work is also a record of what the
  project expected of that work — and this one prejudged both the region target
  and the storage question.
- `docs/status.md`, "What is built": the crate table's `web/` entry gains a tenth
  module; the count of gated corpus checks goes from twelve to thirteen; the
  command list goes from eight to nine.
- `docs/status.md`, O11: a third cache shares `.taiyaku-cache/`.
- `README.md`: the command list.

# The instrument: ground truth, a labeller and a scorer — design

**Status: settled in brainstorming on 2026-08-11. Not yet built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **sub-project A of two**. A builds the thing that can judge a detector;
**B redesigns the detector** and is measured by A. They are separated because B
cannot honestly conclude anything until A exists — see §1.

It closes nothing on its own. O31, O32, O33 and O34 stay open: they are the
detector's, they are held together rather than picked off, and A does not touch
`detect.rs`.

## Where it came from

`status.md` records the detector as the one open item with a code-shaped exit,
and `notes.md` records why the obvious next step is not another candidate. Every
acceptance rule this project has stated for the detector has since failed *as a
rule*:

- **`median_area > 0.15` cannot fire.** `MAX_BLOCK_AREA`
  (`crates/taiyaku-cli/src/detect.rs:500`) discards any block above 0.15 of the
  page *before* a `Region` is constructed, and `median_area` is computed over
  the regions that survive. Corrected in the L2 design on 2026-08-08.
- **`blocks_per_page ≥ 2` fires, and ranks the editions backwards.** The only
  edition of nine to clear it, `gotoubun-v01/en-mono` at 2.50, was found — by
  cropping ten of its blocks and looking at them — to be returning *whole
  panels*. The edition whose blocks are plausibly text, `oshinoko-v01/en-mono`,
  scores 1.40 and fails.
- **L2A Task 9's rule inherited the second one's blindness.** It was fixed
  before measuring anything, which was right, and it was stated in
  `blocks_per_page`, which was the wrong quantity. Both candidates were duly
  refused, and the refusal is not evidence the rule was sound.

The pattern is that the detector has only ever been judged by proxies, and it
took **hand-cropping ten blocks of 76** to find out what the proxies meant.
`notes.md` is explicit that ten of 76 establishes little about the other 66.

**So the fourth acceptance rule would fail the same way as the first three**, and
nobody would find out until someone cropped boxes again. That is what this
sub-project is for.

## 1. Why the instrument is its own sub-project

Because a detector cannot be honestly measured by a rule written in the same
statistics that have already been shown to rank backwards, and because the
labelling is human time that no amount of code substitutes for.

**The alternative it beat: go straight to candidates.** It costs nothing
up front and produces a first candidate sooner, which is a real advantage and
the reason it is written down here rather than dismissed. It loses because the
evidence that the proxy route is exhausted is already recorded: two scale rules
measured against a pre-fixed bar, both refused, and the bar itself later shown
to be blind to the difference between a panel and a caption box.

## 2. What one labelled box is

**One box per text run** — all the text belonging to one balloon, caption,
narration box or sound effect is one box, however many lines or columns it
holds.

This is the unit that survives translation, which is what L2b's pairing needs: a
raw's two vertical columns and the English's three horizontal lines are one box
each, so the pairing stays 1:1.

**Two alternatives it beat.** *One box per line or column* is easier to draw
consistently and less ambiguous at the edges, but it makes a balloon 2 boxes on
the raw and 4 in English, so centroid pairing is n:m and L2b would have to group
before it could pair. *One box per balloon outline* is the easiest of the three
to draw, and the L2 design already refused it for a reason that still holds: a
balloon detector is blind to the borderless narration manga uses constantly, so
that material would get no box at all.

## 3. What a box carries, and what gets scored

Every box carries a **kind**, and an **axis** that may be *not applicable*. The
scorer's headline recall is computed over **all text**, not over dialogue alone.

**This reversed an earlier decision in the same session and the earlier one is
kept here**, because it was argued from the pairing consumer alone. The first
proposal scored dialogue and narration and made SFX, logos and credits *ignore*
regions, on the grounds that an untranslated sound effect has no counterpart to
pair with. That is true and it is still true. It was overruled by a second
consumer: **text selection wants everything a reader might drag over**, which
makes "find all the text" the requirement and "find the pairable text" a subset
of it.

The tags stay, and that is the point of not having collapsed to one class:
**recall is reported per kind**. `dialogue 0.94, sfx 0.31` is visible on every
run, and a later decision to stop caring about sound effects costs no
relabelling.

Scoring all text is the **harder** bar, and it is worth stating that it cuts
against whatever candidate would otherwise win: a detector that cleanly finds
balloons and misses every sound effect now fails on recall where it used to
pass. Given that the best-scoring edition today is returning panels, that is the
right direction.

### The ignore flag

Any box may be flagged `ignore`. The scorer treats it as neither a hit nor a
false positive, and **prints how many it ignored on every run**.

It exists for the genuinely ambiguous tail — signage baked into background art,
text on clothing, marks illegible at scan resolution. The reason is not
statistical: it is that **deciding** "is this selectable text?" twenty-five times
a page is the real cost of labelling, more than the drawing, and without an
escape hatch the job becomes an argument with oneself. The printed count is what
keeps the hatch from being silent.

**The alternative it beat: no escape hatch, everything counts.** Simpler, and
nothing to argue about at label time. It loses because a detector then gets
penalised for missing an 8-pixel shop sign in the background of one panel, which
is not a defect anyone would fix, and that noise lands in the headline number.
A third option — weighting boxes by kind — was refused outright: the weights
would be five more constants chosen before any evidence existed, which is the
exact trap `SMEAR_K` and the other four starting values are already in.

## 4. The document

`taiyaku.truth.json`, beside the sidecar — the convention
`<stem>.regions.json` and `<stem>.notes.json` already use, through the same
arithmetic `RegionDoc::path_beside` uses
(`crates/taiyaku-core/src/regions.rs:132`), including its reason for a
`format!` over two `set_extension` calls.

A new module, `crates/taiyaku-core/src/truth.rs`. It does **not** reuse
`Region`:

```rust
pub struct TruthRegion {
    pub x: f64, pub y: f64, pub w: f64, pub h: f64,
    /// `None` where the question does not apply — a radial sound effect has
    /// no reading direction, and `Region::axis` cannot say so.
    pub axis: Option<Axis>,
    pub kind: Kind,
    #[serde(default)] pub ignore: bool,
    /// Reserved. An OCR pass fills this in later; additive, so no version
    /// bump and no relabelling.
    #[serde(default)] pub text: Option<String>,
}

pub enum Kind { Dialogue, Narration, Sfx, Logo, Credits, Other }
```

**Why a new type rather than a field added to `Region`.** `Region::axis` is
`Axis`, not `Option<Axis>` (`crates/taiyaku-core/src/regions.rs:60`), and
widening it would change what every existing regions document means. `Region`
is what a *detector* emits and `TruthRegion` is what a *person* draws; they are
different claims and the asymmetry is real — a person can decline to answer the
axis question, a detector cannot.

Two things are carried over from `RegionEdition` deliberately:

- **`members: Vec<RegionMember>` and the `is_stale_for` idiom**
  (`regions.rs:95`). Ground truth has the sharpest form of the staleness
  problem in this project. A stale alignment sends a reader to the wrong page;
  stale ground truth draws a box on the wrong art on the *right* page and then
  scores a detector against it. Content hash and order, not paths.
- **`pages: BTreeMap<u32, Vec<TruthRegion>>`, where absent ≠ empty.** Absent
  means nobody has looked at that page; present-and-empty means somebody looked
  and there was no text. Across 75 pages of manual work that difference
  is the progress tracker, and a list of lists cannot express it — the same
  argument `RegionEdition::pages` already makes.

The top level mirrors `RegionDoc` (`regions.rs:106`) rather than inventing a
second shape — a version field and a map keyed by edition, spelled exactly as
the sidecar spells it:

```rust
pub struct TruthDoc {
    pub taiyaku_truth: u32,
    #[serde(default)]
    pub editions: BTreeMap<String, TruthEdition>,
}

pub struct TruthEdition {
    pub members: Vec<RegionMember>,
    pub pages: BTreeMap<u32, Vec<TruthRegion>>,
}
```

`RegionMember` is reused as-is: it holds a `sha256` and nothing else, which is
all the staleness check needs, and a second identical type would only drift.

## 5. The sample: 45 dev, 30 held out

**Dev — 45 pages, exactly.** `region_experiment.rs` samples `SAMPLE = 10` pages
from each of nine editions (`region_experiment.rs:147`) — `gotoubun-v01`
raw-mono/en-mono/en-fan, `kaguya-v01`, `oshinoko-v01` and `kobayashi-v01`
raw-mono/en-mono — which is the 90 pages every figure in `notes.md` is computed
over. The dev set is **every other one of those picks**: indices 0, 2, 4, 6 and
8 of each edition's ten, so 5 per edition and 45 in all.

*This was written as "about 60 pages" and that hid a selection decision, which
the spec self-review caught.* Three properties decided it:

- **A strict subset of the existing 90**, so the new labels sit against
  `notes.md`'s figures with no second sampling rule in between. Calling
  `sample_indices(available, 5)` would *not* be a subset — the formula lands on
  different pages — so the picks are thinned rather than recomputed.
- **The spread is preserved.** `sample_indices` exists because the front of a
  volume is colour inserts, a title page and blank interstitials, the least
  representative pages there are; taking the first six of ten would undo that.
- **Extensible with no rework.** If 45 pages prove too noisy to separate two
  candidates, labelling the other five per edition yields the full 90 and
  nothing already drawn is wasted.

**Held out — 30 pages**, from four regimes the survey has never touched,
all of them already on disk under `corpus/incoming/`:

| Regime | Why it is not in the dev set's four series |
| --- | --- |
| `berserk-cleaned` | dense crosshatching, 1990 print scan — the extreme of ink that is not text |
| `onepiece` v01 | hand-lettered sound effects on most pages |
| `nausicaa` v01 | very dense, hand-lettered |
| `bocchi` `src-hamaji`/`src-krc`/`src-mangazip` | the same volume at three scan qualities — content held constant, which isolates the JPEG-ringing half of the diagnosis |

It is labelled up front, sealed, and **opened once at the end**. It is reported
whatever it says and is never used to choose between candidates.

**Why widen at all.** All four surveyed series are modern, digitally lettered,
similar in tone density — narrow in the one dimension the detector is failing
along. Widening costs labelling time and no new material: the corpus was
measured on this machine at 26 GB, and holds seven series in `incoming/` that
the L2 survey never used.

**Nothing is pulled from the NAS for this.** That was offered and declined:
volume was never the constraint, and the unused material already on disk covers
the missing regimes.

**Estimate, and it is an estimate rather than a measurement:** roughly 15–25
boxes a page under the all-text rule against roughly 8 under a dialogue-only
rule, so on the order of 1,500 boxes over the 75 pages, and about 2¼ to 2¾ hours
of drawing plus judgement. No page has been labelled yet, so this figure has no
measurement behind it and must not be quoted as one.

## 6. The labeller

`web/label.html` → `web/src/label.tsx` → `web/src/Labeller.tsx`.

**A second Vite entry, not a mode inside the reader.** It reuses `Viewport.tsx`,
`PageView.tsx`, `pageSize.ts` and `viewport.ts` unchanged — zoom, pan, rect
mapping and page sizing are L1's shipped work and none of it is rewritten.

*The alternative it beat: a label mode inside `App.tsx`.* That gets the
filmstrip, the keybindings and the settings panel for free, which is a genuine
saving. It loses on three counts: `App.tsx` is already the file
`2026-08-09-app-extraction-map.md` was written about, a labelling mode is a
large amount of state no reader ever needs, and it would ship a development tool
to every reader. *A third option, a standalone offline HTML file with no server,*
was refused because it re-implements zoom, pan and page sizing from scratch and
because most corpus editions are `.cbz` archives that would have to be extracted
first.

**One new shared component, `RegionOverlay.tsx`** — positioned boxes over a page
image, hover and select, zoom-aware. The labeller creates and edits through it;
**the reader later selects through it**. Built once, two consumers. `Annotations.tsx`
already positions overlay markers in page-fraction coordinates and is the
pattern it follows.

**`vite.config.ts` needs `build.rollupOptions.input` added.** It has none today —
verified on this machine — so the single entry is implicit from `index.html`,
and a second entry has to name both.

### Two new server routes

`GET /truth` and `PUT /truth` in `taiyaku-serve`, mirroring the notes routes.
`routes.rs` already has `notes_etag` (`routes.rs:238`) and `If-Match` matching
(`routes.rs:250`) to follow.

**`PUT /truth` must return an `ETag`.** That is O35's defect, and it is named
here rather than left to be rediscovered: a `PUT` response without a validator
turned every later note write into a silent no-op, and the fix was `writeGate`'s
three arms — `wait`, `refuse`, `write`. Ground truth has the same shape and gets
the same guard.

### Interaction

All of it on the image. There is no sidebar box list; that was proposed and
withdrawn.

| Input | Effect |
| --- | --- |
| drag on empty artwork | draw a box |
| hover | that box brightens, the others stay low |
| `1`–`6` | tag kind |
| `v` / `h` / `n` | axis vertical, horizontal, not-applicable |
| `i` | toggle ignore |
| Tab | cycle overlapping boxes under the cursor |
| Delete | remove the selected box |

Boxes sit at low opacity until hovered — **out of the way until you go for
them**, which is the requirement that killed the sidebar. Sound effects
routinely overlap balloons and panel borders, and Tab-cycling is what makes
overlapping boxes selectable without one.

## 7. The scorer

**Five outcomes per truth box, not an IoU pass or fail.**

```
1:1        one prediction ↔ one truth box
merged     its prediction also swallows other truth boxes
split      ≥2 predictions cover it
missed     nothing overlaps it
spurious   a prediction overlapping no truth box   (counted on the prediction side)
```

**Why not IoU@0.5 alone**, which is the obvious choice and what most detection
work uses: the detector's two known failure modes both score as a flat miss
under it, and it cannot tell them apart. `notes.md` records *both* on one
edition at one gap — panels over-merged, and seven "blocks" that are vertical
slices of one word. A single IoU number reports "recall 0.0" for each and says
nothing about which.

This is the region-evaluation scheme from document-layout analysis
(Shafait/Breuel, ICDAR), and the payoff is direct: **it reports by machine what
cost ten hand-cropped blocks to discover.** "This edition is finding panels" is
a high merge count on the first line of output.

Also printed every run: **recall per kind**, and **the ignored count**.

### The one new constant

The overlap fraction at which a prediction counts as covering a truth box.

**It is not guessed here.** It gets settled the way `scripts/region-survey.sh`'s
own header says constants get settled in this repository — by drawing the
proposed matches over the pages and looking at them, not from aggregate output
alone. Until that is done it is a parameter with a stated-provisional default,
and its doc comment says so in the same words `SMEAR_K`'s does. **A constant in
this codebase asserted as measured when it is not remains a defect.**

### Placement

`crates/taiyaku-cli/src/score.rs`. The document format lives in
`taiyaku-core`; the scorer does not, because the browser has no use for it and
putting it there would be speculative.

**Cost.** Roughly 75 pages × ~15 truth × ~25 predicted is about 28,000 box
overlaps — arithmetic, and negligible beside `components`, the flood fill over
every pixel that `region_experiment.rs` already pays for. Scoring adds no human
time at all: it is automatic and re-runnable, and every future candidate is
scored for free. That is the whole return on the labelling.

## 8. The promotion rule, fixed before any candidate exists

> **A candidate replaces the incumbent only if, over the dev set, it increases
> the count of 1:1 matches and does not increase spurious-per-page. Ties go to
> the incumbent.**

A raw count on the first term rather than a rate, because the dev set is fixed
at 45 pages and a fixed denominator makes the two identical; a rate on the
second, because a candidate may legitimately return more predictions in total.

**Relative to the incumbent, not an absolute bar**, and that is the central
decision of this document. Three absolute bars have already failed — one that
could not fire, one that ranked backwards, and one that inherited the second's
blindness. There is no evidence for any particular number, so stating one now
would be the fourth.

**An absolute "good enough to build L2b on" bar is deliberately not stated
yet.** It gets written once the first candidate's numbers exist and the
distribution is visible. Writing it now would be a guess wearing the clothes of
a pre-registered rule, which is worse than no rule because it looks like
discipline.

**The held-out set is opened once**, at the end, and reported whatever it says.
If it disagrees with the dev set, that disagreement is the finding and it is
written up rather than tuned away.

## 9. Testing

**The load-bearing test is that the scorer can see a merge and a split.** Hand-built
synthetic fixtures — a merge, a split, a miss, a spurious and a clean 1:1 — each
asserted to produce its own outcome, before the scorer is pointed at a single
real page. A scorer that silently could not distinguish a merge from a miss would
reproduce exactly the failure this sub-project exists to end, and it would do it
while looking like it worked.

Then:

- **`taiyaku-core`** — format round-trip, `is_stale_for`, `axis: null`
  accepted, an unknown `kind` rejected with a message naming the field.
- **`taiyaku-cli`** — the scorer's outcome classification, per-kind recall
  arithmetic, the ignored count.
- **`web/` jsdom project** — box creation, kind tagging, ignore toggling,
  Tab-cycling order.
- **`web/` browser project** — drag-to-draw geometry. This is precisely what
  that project exists for: `jsdom` has no layout engine,
  `getBoundingClientRect` returns zeroes there, and a box drawn at the wrong
  place is not observable without one. `vite.config.ts`'s own comment makes
  this argument already.
- **Corpus scoring** — gated on `TAIYAKU_CORPUS`, skipping cleanly when unset,
  following `region_experiment.rs`'s shape: assert that it measured something,
  never assert a quality figure. A measurement that fails when the thing it
  measures improves is a check pretending to be a measurement.

## 10. What is deliberately not done

- **Any change to `detect.rs`.** Not one line. That is sub-project B, and A is
  worthless if it is built to flatter a candidate already in mind.
- **OCR.** Real character selection — copy to clipboard, dictionary lookup,
  find-in-page — needs manga OCR, which is a transformer model plus
  Python/PyTorch or a cloud API. That breaks `detect.rs`'s stated constraint
  ("none of them a model and none of them a new dependency") and is a larger
  project than the detector. The `text` field is reserved so that adding it
  later costs no relabelling and no version bump.
- **L2b pairing**, and the peek that depends on it.
- **The reader's selection UI.** Only the shared `RegionOverlay` it will use.
- **Pulling more material from the NAS.** Offered and declined; see §5.

## 11. What could not be measured

- **The labelling estimate in §5** — on the order of 1,500 boxes and about 2¼
  to 2¾ hours — has no measurement behind it. No page has been labelled. It is
  a planning figure and is marked as one.

  *The page count itself is no longer an estimate.* `taiyaku score --sample`
  resolved all fifteen editions against the corpus on this machine and counted
  **45 dev + 30 sealed = 75**. The spec said "about 40" held out before the
  edition list was settled; six editions at five pages each is thirty, and the
  earlier figure was arithmetic done before there was anything to count.
- **The scorer's overlap constant.** By construction: settling it needs labels,
  and there are none yet. See §7.
- **Whether the held-out regimes fail differently from the dev set.** That is
  the question the held-out set exists to answer, and answering it early by
  peeking would destroy it.

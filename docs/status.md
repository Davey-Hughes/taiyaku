# Status

Where the project is, what is settled, and what comes next. Updated 2026-08-13.

The detailed evidence for everything asserted here is in [`notes.md`](notes.md),
which is a working log rather than a summary — every claim below traces to a
measurement recorded there. The format is specified in
[`sidecar.md`](sidecar.md).

## Where it stands

**L0 works.** The aligner reads two editions, pairs their pages, and writes a
sidecar. It has been measured against eleven series and holds up on every case the
design anticipated, plus several it did not.

**The reader now has a front end.** Milestones 1 through 3 shipped: page
reading is a crate of its own, `taiyaku serve` hands an HTTP client the
sidecar, any page by index, and where you were, and it can now read those pages
from a Stump instance as well as local disk and publish an OPDS feed grouping a
work's editions. M5a shipped the browser reader that draws it: single-page,
two-page spread and continuous-scroll modes, a swap between editions that
never writes position, a gap state that names an unpaired slot rather than
silently landing near it — *no longer the default since `gap-fallback`, marked
here rather than rewritten: another edition's page is drawn there and that gap
is one press of its mark away, per the "Fall back to another edition…" entry
below* — and a floating chrome — filmstrip and slot readout — that never
reflows the page underneath. M5d shipped the correction UI: a reader can point
at the right counterpart in the filmstrip and have the fix
written into the sidecar as `by: "hand"`, surviving a re-solve. `verify`'s
contact sheet remains read-only, but it is no longer the only way to see or
change what the aligner decided. A defect M5a had carried since it shipped
closed just after: an arrow steps by what is *drawn* — a group — rather than
by slot, so crossing a joined spread no longer takes two presses and a swap
always answers from the half a reader can see. **M5c shipped annotations, and
with them the one thing no other reader can offer: a note written on the raw
is readable while reading the English**, resolved through the same call the
swap makes, marked in the margin, and listed — including the notes that cannot
be reached from where the reader is standing, which are marked rather than
hidden.

**L1 shipped zoom, and with it the first thing the alignment does for a reader
that a side-by-side viewer cannot do at all.** A reader zoomed into a speech
balloon on the raw presses the swap and lands on the same balloon in English:
the rect they are looking at is mapped onto the other edition's page and the
viewport fitted around the result. A page turn carries the view and re-clamps;
continuous scroll is untouched and keeps fit-whole. **Which transform does the
mapping was measured rather than argued, and the measurement went against the
design**: of the two candidates it named, one turned out worse than doing
nothing, and what ships is a third neither had proposed. §5 below is the record.
Nothing in the sidecar, the aligner or the server changed to make it work.

**Three reader complaints landed on top of it, and the browser corrected two
designs on the way.** Pages fill their box now, the two halves of a spread meet
at the seam, and `?` or `h` lists every key the reader answers — generated from
the same table `intentOf` dispatches from, so there is no second copy to drift.
§6 below is the record, and the parts worth reading are the two failures: the
seam's design closed the gap on the wrong element and a measurement said so, and
closing it introduced a regression that put every annotation marker 376px from
its artwork until the same pass caught that too.

**Two more came from reading in two-page mode, and both were rules that were
right about one page and had never been asked about two.** A pair's halves each
appeared as its own decode settled — measured at a median 89.7 ms apart, every
turn tearing — and they now present together through one barrier. And the two
editions on either side of a swap drew pairs one page out of phase, because
pairing was anchored at slot 0 of each edition independently. **Pair boundaries
are now a property of the work rather than of an edition**: wherever any edition
joins a spread, that boundary is mapped into the hub's slot space, unioned with
every other edition's, and every edition — the hub included — takes its pairing
from the one grouping that results. Swaps that keep the pair intact go from 206
of 580 to 581 of 590. §8 below is the record, and it has two parts worth
reading. What the defect was *not*: the obvious diagnosis was an alignment
error, and a shift test over 12 alignments in 9 volumes says the aligner has
none — after the scan itself was corrected for the one-to-one constraint that
had made four volume tails look broken. And what the first fix missed: deriving
only the *spokes* from the hub scored 96% and left the hub unable to follow a
boundary only a spoke has, which a reader found at one press.

## What is built

Five crates, 741 tests, plus 1,326 in `web/`'s TypeScript — 973 in the `node`
project and 353 in the `jsdom` one — and sixteen more in a
**browser** project of its own, no dependencies beyond
`image`, `zip`, `sha2`, `rayon`, `base64` and `serde`, plus `axum`, `tokio`,
`tower` and `ureq` in the server, `wasm-bindgen` in the binding, `react`
and `vite` in `web/`, and `tract-onnx` as a dev-dependency of `taiyaku-cli`
alone, for phase 3's gated measurement — it reaches no binary and no other
crate. The sixteen run under Vitest's browser mode over Playwright's
Chromium, and cover only what no Node environment can answer: `jsdom` has no
layout engine and no real `HTMLImageElement.decode()`, and every defect the page
area has shipped was one or the other. They are **not** a CI job — a cacheless
CI would download a browser on every run — so `pnpm -C web run test:coverage`, which
is what CI and `check-all.sh` both run, stays the `node` and `jsdom` projects, and
`check-all.sh` runs the browser one separately and locally, naming it
unmeasured when the browser is absent. That adds `@vitest/browser`,
`@vitest/browser-playwright` and `playwright` to `web/`'s dev dependencies;
the runtime list above is unchanged. That list did not grow for L1: zoom is a CSS transform and
a page of arithmetic, and its corpus experiment fingerprints with `image` and
`rayon` exactly as `align` already did. Seventeen of the 741 are standing checks gated on
`TAIYAKU_CORPUS` — ten in `crates/taiyaku-cli/src/corpus.rs`, three in
`crates/taiyaku-serve/src/corpus.rs` and four in
`crates/taiyaku-cli/src/score_corpus.rs` — one of which also needs
`TAIYAKU_STUMP_URL` and `TAIYAKU_STUMP_KEY`; each skips cleanly when what it
needs is absent, so CI stays green without the corpus or a reachable Stump.
One measures composition
against a direct solve — 2,427 page comparisons, 0.91% loss, 0.00% disagreement.
One measures that the server and the aligner agree about what page N is — 617
pages across 3 editions, no disagreement. One measures that a note can be anchored to
every slot of every real edition — 629 anchors across three, two of which join
spreads and so have more slots than pages, which is the case a four-page
fixture cannot tell apart from a page check. One measures that the server and
Stump serve the same bytes for a member both hold — 187 pages compared byte-for-byte,
no disagreement, and 8 members this Stump instance does not hold recorded as
absent rather than silently skipped. Two pin credit-page classification against
both fan editions the corpus holds — Go-Toubun's four pages and Kaguya's single
known false positive, asserted as *sets* rather than counts, since four of six
is reachable by classifying the wrong four; see §4 below. The remaining six run
duplicate resolution over real duplicate sets and are listed in §2 below with
what each one measured. `check-all.sh` runs all twelve with `--nocapture`
against an absolute, canonicalised `TAIYAKU_CORPUS`, so a green gate shows what
each one measured, or says plainly that it did not run them at all. **It runs
them under `--release`**, and its `test` job runs with `TAIYAKU_CORPUS` removed
from the environment so they run once rather than twice — the whole gate is
about 15 seconds warm, against a run that was killed at 1h09m before that split.
The debug profile is the only one with overflow checks and `debug_assert!`, so
the unit tests are what exercise those and `--corpus-debug` puts the corpus
checks back under them; `docs/ci.md`, "Two profiles, and the gate that took an
hour", is the record.

**Seven further tests are gated on `TAIYAKU_CORPUS` and are deliberately not
among those twelve.** All seven are one-off measurements rather than checks, and
all seven are built the same way for the same reason: gated, skipping cleanly
when the variable is unset, calling the detector's own functions rather than
copies of them, and asserting only that they measured something — never a
quality figure, because a check that fails when the thing it measures improves
is not a measurement. **Three of the seven are phase 3's**, all in
`crates/taiyaku-cli/src/model_experiment.rs` and additionally gated on
`TAIYAKU_MODEL`: the head-to-head table against `detect_page`; — added
2026-08-12 — the measurement of whether the model groups a page's balloons the
same way on the raw and on the English edition, which is a pairing question
rather than a labelling one and reads no truth boxes at all; and, the same day,
a sweep of the model's confidence floor `BLK_CONF` over eight values, which is
`band_sweep.rs`'s shape applied to a constant taken verbatim from upstream and
never measured here. All three are described in the L2 entry below. This
sentence said "four" until the second landed, having missed the first one; the
count is not gate-checked, only the two totals above are. Two more are
described below; the remaining two are L2b's, and are
`crates/taiyaku-cli/src/truth_population.rs`, which measures what text is made
of from inside 478 hand-drawn truth boxes, and
`crates/taiyaku-cli/src/band_sweep.rs`, which scores the detector at eight
values of `BAND`'s `lo` over all 45 labelled pages and is what fixed it. Both
read their pages through `score::resolve_labelled`, so they measure exactly the
pages `taiyaku score` scores. `notes.md`, "What text is made of, measured
against 480 hand-drawn boxes" and "Where the band goes, measured at eight bands
over 478 hand-drawn boxes", are what each one printed. The band sweep is about
51 seconds under `--release`: it detects all eight bands inside one decode pass,
where re-entering the decode per band would be 360 full-resolution decodes to
score 45 pages.

`crates/taiyaku-cli/src/region_experiment.rs` is L2a's: why the text-block
detector fails, run against real pages. It prints, per sampled page, the
component count, the fraction of components under 5px, the scale each of three
candidate rules derives, the gap that follows, **how many blocks survive each of
the four filters in order**, the largest block `MAX_BLOCK_AREA` discarded, and
the axis flagged against the edition's true reading direction — and, since the
measurement below that asked what the one edition clearing the bar was actually
finding, **what every surviving block is made of**: its size, how many
components it holds, how many of those are under 5px against how many of the
page's are, and their median size, plus the page's own pixel dimensions, the
smear gap as a fraction of them, the largest component *before* any smearing,
and what each of the four filters rejected across the whole sample. It exists because
the first version of this diagnosis was a throwaway program, discarded once
`notes.md` was written — which left the survey's `regions:` lines reproducible
and the diagnosis behind them not. It calls `detect.rs`'s own functions rather
than copies of them, so nothing in it can drift from what `taiyaku regions`
does; the discarded program copied them and needed a cross-check against the
real command to show the copy was faithful. About seven seconds under
`--release`. See `notes.md`, "The text-block detector, measured against four
works, and why it does not work yet", "The scale rule, tried three ways, and
neither candidate clears the bar", and "The edition that clears the bar is the
one finding no text" — the last of which names the command that reprints every
figure in it.

`crates/taiyaku-cli/src/geometry_experiment.rs` is L1's one-off
measurement of which derived transform a swap should map through, and it stays a
measurement rather than becoming a check: it asserts only that it measured
something — a non-empty sample, a search that found a minimum in every volume,
every residual finite — so that a glob matching no volume cannot print an empty
table and pass, which is the one failure a measurement must not have.
`check-all.sh` reaches none of these four — its corpus job filters on
`corpus::`, and its `test` job now runs with `TAIYAKU_CORPUS` removed from the
environment — so the twelve above are still twelve. All four are excluded from
the per-file coverage floor by name, in the `--ignore-filename-regex` that
appears twice in `check-all.sh` and twice in `ci.yml`; a gated experiment that
skips in CI would otherwise report near-zero coverage and fail the floor. That second half is new:
until 2026-08-08 the `test` job inherited the variable and ran both of these in
**debug**, which is how a gate run came to be killed at 1h09m inside
`geometry_experiment` alone. This one also wants `--release`, under which it
takes about a minute against about forty without. Not promoting it was decided in
advance, by a rule fixed before the number existed (the L1 design's §4), and
the rule's first branch fired. It also could not have been promoted as written:
the transform it picked is TypeScript, so a Rust corpus check would re-measure
page dimensions no code in this repository can alter and go green while the
browser did something else. The unit tests beside `mapRect` are what guards
`mapRect`. See `notes.md`, "The exposure, and why there is no standing check".

`taiyaku-core` is the portable half — no I/O, so it compiles to WebAssembly:

| Module | Holds |
| ------ | ----- |
| `solve` | similarity, per-pair floor calibration, Needleman-Wunsch, run compression, confidence, verdict |
| `slot` | spread detection, slot expansion, reading direction |
| `sidecar` | the `taiyaku.json` format — parse, emit, judge, merge an edition in |
| `route` | which path a lookup walks between two editions, and the page it lands on |

Three of the other four touch the disk or the network. `taiyaku-wasm` touches
neither — it is portable like the core, and separate from it so the core's API
is not shaped by what can cross the JavaScript boundary.

| Crate | Holds |
| ----- | ----- |
| `taiyaku-read` | how a member becomes pages — archives, directories, natural order, page counts, content hashes, one page by index |
| `taiyaku-serve` | the HTTP API: works, sidecar, page by index, progress, notes (`<stem>.notes.json`, read and written whole under `If-Match`), an OPDS 1.2 + PSE feed and generated covers, all behind a `PageSource` trait with **two** implementations — local disk, and a composite that resolves each member to disk or, inline within the same type, to Stump (see below). A caching split runs the length of the file: the four routes whose bytes are fixed by a member's `sha256` carry a day's freshness — two of them an `ETag` as well, and the thumbnail and cover neither, since both find the member only inside `spawn_blocking`, too late for a validator to save the work it would be skipping — while every route answering a document this server rewrites carries no freshness at all, on the `304` as well as the `200`, which is the half that is easy to get wrong (O1) |
| `taiyaku-wasm` | the binding a browser reader loads: parse a sidecar, resolve a slot between editions, and say which path the answer walked |
| `taiyaku-cli` | the `taiyaku` binary: image decoding, fingerprinting, the content-addressed cache, the HTML report, argument parsing, and duplicate resolution — a copy's signature, the all-pairs judgement over one set, and the inventory walk that finds the sets |
| `web/` | the browser reader (React, Vite): single page, two-page spread and continuous-scroll modes, an arrow that steps by what is drawn rather than by slot, a swap that never writes position, a slot the edition being read has no page for filled from another edition and marked as borrowed (the gap state that used to stand there, offering the nearest paired slots either side as moves rather than jumps, is one press of that mark away), and a floating chrome — filmstrip and five independently-switchable readouts (position, reading time, chapter, alignment health, note count) — behind the `Resolver` interface `taiyaku-wasm` implements. Every reading behaviour not yet in this list is one of ten `ReaderModule`s — of thirteen the registry holds — against a single declarative contract (`module.ts`): `direction` (a per-edition reading-direction override), `motion` (animate on/off, transition duration), `regroup` (parity breaks a reader inserts by hand), `readouts` (which of the five lines show), `prefetch` (depth ahead/behind plus a double-buffered, decode-before-swap image so a fast flick presents no blank frame), `annotations` (notes anchored to a slot of one edition and travelling to every other through the alignment, several to a page, each in a colour of the reader's choosing — a name in the file, never pixels — drawn as margin tabs plus an on-demand region tint — the tab in the letterbox beside the page, moving to the *outer* margin in two-page mode since closing the seam leaves a page no margin on its seam side — with five keys — write, reveal, review, and step back and forth through what has been written — and revealing off by default, since the margin tabs already say where the notes are without drawing on the page; notes save themselves a moment after the last change rather than on a button, so `history.ts`'s snapshots are what a reader abandons a change with; a region is swept out with a drag and pushed around with another, and while the editor is open every region on the page is visible and grabbable with one Save writing all of them, on an overlay that measures the artwork it sits over rather than the element around it — the same box, until `fit` made the element the whole cell and the two routinely different (§6 below); a note anchors to the whole slot, a point or an area, and its words are readable in a numbered list beside or below the page as well as in the region itself), `fit` (§6 below: how large a page is drawn inside its own box, and how far the two pages of a spread sit apart — `upscaleToFit` **on** by default, reversing the M5 design, so a 685×1024 scan fills a 1263px box instead of sitting at 1:1 in the middle of it, with `upscaleFilter` exposed beside it because that is a 23% enlargement of line art; and `gap` at `0`, where it is the two pages' *artwork* that meets rather than their elements — a distinction the design got wrong and a browser corrected), `layout` (which of the three reading modes is drawn — a module purely so that it *persists*, since it was component state and so was forgotten on every reload), `zoom` (§5 below: free continuous zoom and pan in single-page and two-page mode — wheel or pinch anchored at the pointer, a drag to pan with the note editor closed or a modifier held, and `=`, `-` and `0`; the view is one rect in *group* space, since a viewport can straddle both halves of a pair, and it is drawn as a single CSS transform on a wrapper, so `PageView`, `paintedBox` and every existing measurement are untouched — by zoom, which is what this claim was about; `fit` later changed both, §6; a page turn carries the rect and re-clamps, and a swap carries the visible content onto the other edition's page through the derived transform, or throws it away, a preference either way; continuous scroll keeps fit-whole, deliberately — its zoom is a different mechanism, since the strip's layout height is a function of scale and pan competes with scroll rather than sitting beside it), and `borrow` (the "Later" entry below is the record: where the viewed edition has no page for the slot, another edition's is drawn there and marked, and the mark is a button revealing the `Gap` view it replaced — hub-first order minus the viewed edition and minus the authority, with the authority lending when none of the rest resolves, which it always can; on by default, so a gap now reaches a reader only through that mark or with this switched off). The other three registry entries claim no reading behaviour. `settings` claims only the panel's own key. The remaining two are panels rather than page treatments. `credits` (§4 below): `C` lists the viewed edition's credit pages, a thumbnail and a page number each and no labels the sidecar cannot support, and `skip` keeps them out of the reading flow by being handed to `groups` rather than filtering its result — so the page area, the arrows and the filmstrip skip together, and two-page mode re-pairs around a skipped slot instead of stranding its neighbour. `help` (§6 below): `?` or `h` opens a list of every key the reader answers, generated from `keys.ts`'s one ordered table and from the registry rather than written out — so a binding that changes what it does cannot keep an old description, because there is one description — and listing only the modules that are switched on. `Settings.tsx` is generated from each module's `defaults` — a value's runtime type decides its control, never the module's id, and a string a module declares `options` for becomes a select — with a reset per section |

`taiyaku-read` exists because the page sequence is normative — `sidecar.md`, "How
a member becomes pages" — so two consumers deriving it separately is a defect the
format cannot detect. The aligner and the server now derive it in one place, and
a gated corpus check compares all 617 pages of the primary volume across the two
paths.

Nine commands:

```
taiyaku align  <a-member>... -- <b-member>...   [--ltr] [--cache DIR] [--stats] [--write FILE [--title NAME] [--name-a KEY] [--name-b KEY] [--direct]] | --no-split-spreads [--stats]
taiyaku verify <a-member>... -- <b-member>...   [--out FILE] [--sidecar FILE [--cache DIR] [--ltr]]
taiyaku topology <hub>... -- <spoke>... -- <spoke>...
taiyaku lookup <taiyaku.json> <from> <to> <page>...
taiyaku serve  <taiyaku.json>...                [--addr HOST:PORT] [--stump-url URL] [--stump-key KEY] [--log-requests FILE] [--refresh-stump] [--cache DIR]
taiyaku inventory <root>                        [--min-pages N] [--pattern SPEC]
taiyaku duplicates <copy>... -- <copy>...       [--against <member>...] [--cache DIR] [--ltr]
taiyaku credits <member>...                     --sidecar FILE [--against EDITION] [--cache DIR] [--write]
taiyaku regions <member>...                     --sidecar FILE [--write]
```

`align` takes several members on either side, which is how one volume aligns
against seven chapter files, or two tankōbon against one deluxe volume. `--write`
**merges** into an existing sidecar rather than overwriting it, which is how a work
gets a third edition; on a merge the a-side is the document's hub, so `--name-a` is
optional and an error if it names anything else. `--direct` overrides that for a
pair that has earned a direct edge. `verify` writes a contact sheet for looking at
what the aligner decided. `lookup` resolves a page through a written sidecar, which
is the operation a reader performs — through the hub where no direct edge exists,
saying on every call which path it walked. `serve` is handed sidecar paths the
same way `align` is handed members, because deciding which files constitute a
library is a different job; it serves `/api/works`, the sidecar verbatim, a page
by index, and a reading position, now accepts a hand correction — `PATCH
/api/works/{work}/alignments`, guarded by `If-Match` against an `ETag` on the
sidecar — and 404s with a message naming what was missing rather than
returning a blank page. `--stump-url`/`--stump-key` (or their
`TAIYAKU_STUMP_URL`/`TAIYAKU_STUMP_KEY` env fallbacks) point it at a Stump
instance; pages come from whichever source — Stump or local disk — actually holds
the member, resolved once at startup and cached beside the sidecar. An OPDS 1.2 +
PSE feed at `/opds/v1.2/catalog` groups a work's editions under one entry, which
is what Stump and Komga cannot express on their own.
`--stats` prints what calibration measured and what shape came out — noise,
signal, separation, margin, run count, average run length and the verdict — one
line each, in a fixed field order so a survey can collect them into a table.
`scripts/margin-survey.sh` is that survey.

`inventory` and `duplicates` are the two halves of deciding which of several
copies of one volume to keep, and **neither of them moves, renames or deletes
anything**: they print what they found and leave the acting to a person.
`inventory` walks a release directory and takes as one candidate copy the
deepest directory that still holds a whole edition, so a volume whose chapters
sit in subdirectories counts once rather than seven times; it groups the
candidates by the volume number in the name — `v077`, `第077巻`, or a
`--pattern` where that fails — and prints every group with more than one copy,
together with what a content signature already settles about it. A group whose
name it could not parse is printed whether or not it is duplicated, because a
candidate that reached the page threshold and then vanished from the output is
the silence this whole project is written against. `duplicates` takes those
copies as `--`-separated groups, exactly as `align` takes members, and runs the
protocol over one set: signature and content hash first, then every unordered
pair solved once, then a clustering on identity, a flag on anything that refused
against every other copy, and a ranking of what is genuinely distinct. **Where
`--against` names a reference edition the reference does the flagging instead**,
because a copy that pairs with the reference is this volume however the set
treated it, and that is the only thing that can tell two copies refusing each
other apart. It prints
a table per set for a person and one fixed-field `set:` line for a survey, the
discipline `--stats` established; `scripts/duplicate-survey.sh` is that survey.

`credits` keeps that same discipline for a third question: which pages of an
edition are the scanlation group's own matter — donation cards, staff credits —
rather than the work. It proposes and prints; it writes the edition's `credits`
list only under `--write`. **Only one of its two signals classifies.** Signal 1
is `unpaired_runs`: the stretches of one member's pages the alignment against
the counterpart never reaches, split at member boundaries, and a run is a
candidate when it ends at its member's *last* page and is no longer than
`MAX_CREDIT_RUN`. Signal 2 is `recurrences`, how close each page comes to a page
in a different member by dhash; it is printed beside every row and consulted by
nothing, because on `corpus/gotoubun-v01` its two tightest matches in the whole
edition are the volume cover and it misses both staff-credit pages entirely.
The end-of-member rule is four for four on that volume and the first-index
version is zero for four — what accumulates at a member's start is front matter.
`MAX_CREDIT_RUN` is **1, measured** — surveyed over both fan editions the corpus
holds, 461 pages and four scanlation groups, where every credit page found is a
run of exactly 1 and the only run of length 2 or 3 is a volume's front matter.
`scripts/credit-survey.sh` is that survey. It cost the four-for-four claim above:
`kaguya-v01`'s one candidate is a **false positive**, a publisher's colour
announcement, so precision across the corpus is four of five. That group prints
no credit page at all. See `notes.md`, "A second group's habits, and what the
bound is worth".

`regions` keeps that discipline for a fourth question and is **the first command
here whose answer is not yet good enough to use**. It proposes the text blocks
of one edition — page by page, in page fractions, with the axis the text runs
along — and writes them, under `--write`, to a sibling document
`<stem>.regions.json` rather than into the sidecar: regions are per page, and an
edition of 195 pages at a handful of blocks each is two orders of magnitude more
JSON than a file whose first two design goals are human-readable and diffable.
The document is the convention `<stem>.notes.json` and `taiyaku.stump.json`
already use, and an L0 reader does not merely ignore it — it never fetches it.
**Which block corresponds to which block on another edition is not stored**, and
that is deliberate: it is derived at read time from the alignment, so a third
edition costs one detection run rather than a re-pairing, a hand correction
re-pairs for free, and no stored derivation can outlive what it derived from —
which is O24's open failure mode. There is deliberately no `--cache`: the
fingerprint cache holds hashes and dimensions rather than pixels, so it could
not take effect, and a flag that cannot take effect is refused rather than
ignored. `scripts/region-survey.sh` is its survey. What that survey found is
below, under "What is known to be wrong or missing".

- **The web toolchain is pnpm, Node 26 and TypeScript 7** (2026-08-09). A
  toolchain branch that touched no Rust and no reader behaviour. Three things
  came out of it worth keeping: pnpm rejects the gitignored `file:` dependency
  outright and resolves it to a stale hard-linked copy when it does not, so the
  protocol is `link:`; the `wasm` job had been running the compiled module on
  Node 18 because that is what Debian bookworm ships, against 22 everywhere
  else; and `check-all.sh` now checks that `package.json` and the lockfile
  agree — measured across three runs at 249ms, 239ms and 237ms, a gap
  `docs/ci.md` had carried as permanent. TypeScript 7's `tsc` is a Go binary,
  which took the typecheck from 1.511s wall (3.19s CPU) to 0.164s and 0.166s
  across two runs (0.93s and 0.99s CPU), with no script change. bun was
  measured against, not merely dismissed: the whole JavaScript gate is about
  2.5 seconds, so there is no runtime budget to recover, and the one place a
  faster engine could apply is vitest, which bun cannot run in browser mode.
- **`clippy::pedantic` runs, advisory and non-blocking** (2026-08-10). This
  step first shipped carrying 100 per-site `#[expect(clippy::…, reason =
  "…")]` attributes; a same-day review then asked whether they could be fixed
  rather than documented; and a follow-up campaign fixed 60 of them outright
  — 3 `clippy::similar_names` renames, a `Pairing` enum replacing
  `Option<Option<String>>`, `SlotSpan` gaining its own serde impls, and 55
  casts routed through named `try_from(..).expect(..)` helpers that argue
  their bound once rather than at every site (`docs/notes.md` has the
  detail). The workspace now carries **40** per-site `#[expect]` attributes:
  33 on individual casts — 25 `clippy::cast_precision_loss`, for which no fix
  applies (all three routes measured; see `docs/notes.md`) — and 8
  `clippy::cast_possible_truncation`/`clippy::cast_sign_loss`, each kept for a
  reason specific to its site (four in `taiyaku-wasm`, where a panic traps the
  whole module rather than unwinding; two in `taiyaku-serve` tracing to
  request-path rather than in-process data; one each in `taiyaku-core` and
  `taiyaku-cli` with no viable conversion route) — plus 7 function-level
  (`clippy::too_many_lines` ×3, `clippy::needless_pass_by_value` ×3, and one
  `clippy::many_single_char_names` on `solve`, which judges a whole function
  rather than one binding). Plus two module-level
  `#[allow(clippy::pedantic, …)]`s, both on the one-off
  `*_experiment` modules and argued at the declaration, the sole exception to
  the per-site rule. `.forgejo/workflows/ci.yml`'s `lint` job and
  `scripts/check-all.sh` both run `cargo clippy --workspace --all-targets
  --all-features -- -W clippy::pedantic` after the blocking gate —
  `continue-on-error: true` in the first, `|| true` in the second — and both
  print, unconditionally, 15 `clippy::float_cmp` warnings from `assert_eq!`
  on floats in tests that `scripts/pedantic-sites.mjs` correctly excludes as
  not belonging to any file in this repository (see `docs/ci.md`, "The
  pedantic lints, and why they are advisory"). **The two halves of that
  mirror do not carry equal evidence.** `|| true` is a shell operator,
  verified on every local run of `scripts/check-all.sh`: the advisory step's
  own exit code is always swallowed, and there is nothing to check on a
  Forgejo runner that this repository's own commands can prove one way or
  the other. `continue-on-error: true` is Forgejo Actions' own key for the
  same behaviour, no other job in `ci.yml` uses it, and — since it can only
  fire when the step it guards actually fails — verifying it needs a step
  that fails on a pushed branch, not a local read. The branch has since been
  pushed and the pipeline came back green, but that does not verify the
  key: `cargo clippy -- -W clippy::pedantic` exits 0 regardless of how many
  warnings it prints — only `-D warnings` sets a non-zero exit — so the
  advisory step succeeded on its own and `continue-on-error` was never
  exercised. More fundamentally, the step cannot fail from a lint at all:
  its only possible failure is a genuine compile error, which the blocking
  clippy step above would already have caught, so both guards are
  belt-and-braces for a failure lints cannot produce today. The key is
  untriggered rather than untested; it has still never been observed doing
  its job.

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
- **Fragmentation is judged on average run length**, not runs per hundred pages.
  Measured across the corpus, every confirmed-good pairing runs 36.0 to 200.0
  slots a run and everything that must not pass 6.8 or below, with no overlap;
  the threshold sits at 27.0, three quarters of the weakest good figure, capped
  by the edition's own length because nothing shorter than 146 slots has been
  measured. Both measures separate on this corpus once refusals are gated on
  coverage; average run length was chosen because it states the property in
  slots rather than per hundred pages, so it does not shift with the edition's
  length.
- **Slots, not pages, as the addressed unit.** A slot is a whole page or one half of
  a joined spread. Spreads expand before solving, so the alignment stays strictly
  1:1 and the solver never learns they exist.
- **Reading direction is an input.** Monotonicity forbids reversing a pair, so it
  cannot be discovered.
- **Hub-and-spoke topology**, raw as hub, validated against all-pairs at ~0%
  disagreement, and **resolvable** — a lookup with no direct edge composes
  through the hub. Measured on the path a reader actually walks: 0.91% loss and
  0.00% disagreement over 2,427 comparisons on Go-Toubun v01.
- **One stored edge per spoke, reversed when needed.** The solver is symmetric on
  the pairing relation, measured across five pairs in two series, so N−1 edges
  carry every ordered pair rather than 2(N−1).
- **JSON for the sidecar**, with span notation (`7`, `4R`, `2-5`, `45R-46L`).
- **How a member becomes pages is part of the format, not an implementation
  detail.** The page sequence is derived, so a reader deriving it differently
  disagrees with every run while still matching every `sha256` and `pages` count
  in the file. Entries sort by whole path in natural order rather than as
  stored; non-images are not pages. Measured over 57 archives: getting the first
  wrong changes the sequence for 32 of them, the second shuffles chapters
  together on 2 of the 7 that nest, the third shifts everything after a
  `ComicInfo.xml` on 12.
- **A content-addressed fingerprint cache** keyed on member SHA-256. 3.4 s → 189 ms.
- **`pillarbox` as the transform a swap maps the visible rect through** — the
  shared artwork is as tall as the taller page says, and the wider-aspect page
  carries its extra as margin at left and right. Measured over the 284 slots
  whose search settled, of 288 sampled from twelve corpus volumes across four
  series: median residual **0.0195 of page width and 0.0041 of page height**,
  p90 0.0276 and 0.0116. It beat `identity` (0.0296 / 0.0041 — right on the
  vertical, uncorrected on the horizontal), `letterbox` (0.0296 / 0.0162, which
  is **worse than doing nothing**: it never rescales the axis that needed
  correcting and adds four times `identity`'s vertical error to the axis that
  did not) and reset-on-swap, which is a half-page bound rather than a residual.
  Neither of the two candidates the design named won, and `pillarbox` was not
  one of them; §5 below and `notes.md`, "Which derived transform maps a rect
  between editions", are the record.
- **Coverage thresholds set at what was achieved, minus two points**, and
  proven by regression rather than asserted. Web: 96/92/97/97 under a measured
  98.03% statements, 94.38% branches, 99.29% functions, 99.53% lines. Rust:
  `--fail-under-lines 93` under a measured 95.75% with four files ignored —
  87.59% without them, a difference that is exclusions rather than work — plus a
  **per-file floor of 57% lines** under a lowest measured 59.44%
  (`taiyaku-serve/src/stump/http.rs`) and a **minimum of 32 files** against 37 in
  the report. Both gates were deliberately regressed: `#[ignore]` on `main.rs`'s
  84 tests takes the workspace to 82.19% and exits 1; skipping
  `viewport.test.ts` takes the web figures below all four thresholds and exits 1.
  It beat picking a round number up front, which is either unreachable or
  vacuous, and it beat gating Rust regions as well as lines, which needs a second
  number maintained for no extra signal. The Rust per-file floor also beat
  `cargo llvm-cov --fail-under-file-lines`, which exists in the pinned 0.8.7 and
  was measured exiting 1 with the ordinary coverage table on stdout and **zero
  bytes on stderr** — naming neither the file that tripped it nor the threshold,
  which above a 37-row table is not a gate anyone can act on. Its cost is a node
  install in a Rust container.
- **The regression proof found a hole in the gate it was proving, and the gate
  grew a second half because of it.** Skipping the whole of `pairing.test.ts`
  takes that file from 100% to 83.58% and the total only 98.03% → 97.49% —
  because `App.test.tsx` covers the same lines through a mounted reader — and
  **every aggregate threshold still passed**. A developer could delete a unit
  suite and be told nothing. `scripts/coverage-floor.mjs` is the answer: a floor
  per file, two points below each metric's lowest measured figure. It exits 1 on
  exactly that regression, naming the file and all three metrics that fell.
  vitest cannot express both halves itself — a glob entry in `thresholds` is
  aggregated over its matches, and `perFile` inside one is ignored; both were
  tried and measured before the script was written. **One script, two report
  formats, both gates**: it reads istanbul's `coverage-summary.json` for the web
  half and `cargo llvm-cov report --json` for the Rust half, detecting which by
  shape rather than by filename, and a report matching neither exits 2 rather
  than passing. One file beat two because the two checks are the same idea twice
  and only the adapter differs; the cost is that the differences between the
  formats — llvm-cov reports 0% for an empty metric where istanbul reports 100%,
  or the string `"Unknown"` on a collapsed report — all live in one table.
  `docs/ci.md`, "The coverage gates", is the record.
- **A minimum file count, because both halves used to pass on a report with no
  files in it.** Measured: with `coverage.include` pointed at a directory that
  does not exist, vitest prints `Unknown% ( 0/0 )` for all four metrics and
  **exits 0**, and the floor script printed "0 files, all above the floor" and
  exited 0 too — so a typo in one glob left the whole gate green while measuring
  nothing. `minFiles` is 50 for web (57 measured) and 32 for Rust (37). **50
  rather than 40**, and that is the measurement worth keeping: deleting one `*`
  from the include glob — `['src/*.ts']` — drops all 13 files of `src/modules`
  and vitest reports 97.79/93.90/99.20/99.48, every threshold passed, exit 0. The
  per-file floor sees nothing either, because a file that is absent is a file no
  per-file check visits. Only a count can see it, and 40 would have caught the
  bare collapse and let that one through. On the Rust side the same check
  replaces a ritual `ci.yml` used to ask a reader to perform by hand — "check the
  file count when adding a crate". The cost is that a deliberate exclusion can
  trip it; the rule is to re-derive the number beside the exclusion rather than
  widen it on reflex.
- **The test count in this file is derived by a gate, because by hand it went
  stale twice in one day** — 639 stated against 656 actual, then 656 against
  658, each caught only because someone happened to look. `scripts/status-test-count.mjs`
  compares both stated figures against the JUnit report `cargo llvm-cov nextest`
  already writes — one line in `.config/nextest.toml` turns it on — so the check
  adds no build and no test run, and it runs in the `test` job and
  `check-all.sh` alike. **It cost 14.3 s a push until it did.** The first
  version shelled out to `cargo nextest list` for a *static* enumeration, which
  is genuinely the more robust source — it reads no environment and cannot be
  perturbed by a failing suite — and rebuilt the entire workspace to get there,
  since `llvm-cov` builds with instrumentation and a plain `list` cannot reuse
  those artifacts. That is 31% of the `test` job to check one number, and the
  three reasons written down for paying it did not survive being looked at:
  two output shapes is a local-fallback problem on the one path where the check
  is skipped anyway; the environment cannot reach a run both callers make with
  `TAIYAKU_CORPUS` removed; and a red suite is now *handled* — the script
  refuses to compare and names the failures — rather than avoided. The design
  was chosen first and defended afterwards, which is the reusable part. Proved by
  regression six ways: a stale figure and an added test each exit 1 naming the
  line, and a reworded claim, a run with failures in it, a zero-test report or a
  wrong shape each exit **2** rather than passing. It gates only the total,
  deliberately: the "thirteen standing checks" beside it is correct but
  separable only by a name prefix, and a gate resting on a naming convention
  would start lying silently. `docs/ci.md`, "The one documentation claim a gate
  can check", is the record.
- **`clippy::pedantic` is on as an advisory step, never in `[workspace.lints]`.**
  420 sites measured at `df8cbd9`; 375 fixed or given a per-site
  `#[expect(…, reason = …)]` stating the bound, and 45 in the two one-off
  experiment files covered by a module-level allow, argued at the declaration.
  It beat a blocking gate, which a floating toolchain turns red on an unrelated
  PR every six weeks or so; and it beat leaving the lint off, because `as` is
  invisible to every other instrument here — `docs/ci.md` records the debug
  profile as the only one with overflow checks, and overflow checks do not apply
  to `as`. The enforcement that remains is narrow and real: a stale `#[expect]`
  is `unfulfilled_lint_expectations`, an ordinary rustc lint the existing
  `-D warnings` gate already denies, so a cast deleted without its attribute
  fails CI.
  **The design's own mechanism for switching two lints off by name does not
  work, refuted rather than repaired** — a `[workspace.lints.clippy]` allow is
  overridden by the trailing `-W` of the pedantic group, measured directly:
  with `must_use_candidate = "allow"` in the table, all 47 of its sites still
  fired and the total stayed at 420. So both lints §10.1 would have switched
  off by name were *fixed* instead, and the sites supported that better than
  the design's own argument for switching them off did: all 47
  `must_use_candidate` sites were pure functions where discarding the result is
  a live bug, not the false positive the design assumed, and all 39
  `match_bool` sites were two-arm `match <bool> { true => …, false => … }`, not
  the exhaustive enum dispatch the design's argument described.
  **The branch found one live defect in the 131 casts it read** —
  `Vec::with_capacity(entry.size() as usize)` in `taiyaku-read`, twice, in
  `read_archive` and `member_page`. A `.cbz`'s central directory declares its
  own uncompressed size, unchecked against the real bytes until they are
  inflated; measured with a patched header, `entry.size()` reported 2 GiB
  against 104 actual bytes, attacker-controlled since `taiyaku serve` reads
  archives from requests. Bounded now by `MAX_ENTRY_CAPACITY_HINT` at 32 MiB —
  a whole-branch review found the first-cut 1 GiB ceiling reserved
  1,073,741,824 bytes for a 6-byte entry, and found the regression test that
  should have caught it self-referential (`capacity() <= MAX_ENTRY_CAPACITY_HINT`
  passes for any ceiling at or above the declared lie). 32 MiB is sized from
  the corpus itself — real pages run 100 KB–3 MB, a live client session
  transferred at 247 KB mean and 2.0 MB largest — not from
  `taiyaku_read::MAX_DECODE_ALLOC`, the separate ceiling that bounds a
  *decoded* pixel buffer rather than this *encoded*-bytes hint, and it matches
  `taiyaku-cli`'s independently-chosen `LARGE_PAGE_BYTES` on the same corpus.
  The regression test now asserts a literal reservation bound rather than the
  constant itself, and the identical clamp on `member_page` — the path
  `taiyaku-serve` actually calls per page request — has its own test too. The
  design read ten of the
  highest-risk sites before writing this plan and found zero live defects; the
  branch's full pass read 131, thirteen times that sample, and found this one.
  That is the result at 13× the sample the design's zero was drawn from, worth
  stating as a result rather than folding into the arithmetic as a footnote.

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
| **All-spread edition ↔ ordinary** (One Piece v092) | 1 run, all 200 slots of the ordinary copy — which lies wholly *within* the all-spread copy's 202 |
| Unrelated volumes | refused |
| **A mislabelled file** (Bocchi *Gaiden* filed as v01) | refused, and its real counterpart found |
| Art book ↔ manga | pairs everything and should not — see below |
| **Twenty-two needles** (calendars, Jump issues, two art books) | 7 clear calibration and pair 100%; all 7 flagged `FRAGMENTED` |

## What is known to be wrong or missing

Stated plainly because each is a thing a reader would hit.

**A small needle in a large haystack always aligns — but `verdict()` already
catches it, and the tool was not saying so.** When side A is much shorter than
side B, the DP inserts gaps in B freely, so A comes back fully covered no
matter how unrelated the two are. Measured across twenty-two needles — One
Piece calendars, weekly Jump issues carrying chapters no volume in the pair
contains, and two art books — seven cleared calibration, and every one of the
seven paired 100% of its pages with nothing unmatched.

**Neither calibration statistic can refuse them, which is now measured rather
than suspected.** A One Piece art book against Go-Toubun no Hanayome — a
different series by a different artist — posts a margin of 0.020, beating four
confirmed-good pairings, and a separation of 0.145, beating seven. So margin is
not a weak signal but an inverted one, and `MIN_SEPARATION` cannot be raised to
catch needles without refusing genuine volumes. No margin guard ships.

What does separate is run structure: every needle that got through averages 1.1
to 1.7 slots a run against 36.0 to 200.0 for every confirmed-good pairing.
`verdict()` calls all seven `Fragmented`, and both `align` paths now print that
verdict rather than one of them discarding it. **The consequence for a reader is
that acceptance is a statement about calibration, not about the alignment —
only the verdict is the latter.** See "Twenty-two needles" in `notes.md`.

**Art books cannot be aligned and the solver does not say so.** Reading them works.
Linking their pages to the manga pages they came from is a different problem —
sparse, unordered, cross-composition — and wants nearest-neighbour search with a
margin threshold plus a crop-tolerant hash. Not the DP.

**`taiyaku regions` proposes text blocks and does not reliably find them.** The
command, the document format and the survey all work and are tested; the
detector behind them does not clear the L2 design's own bar. Measured over nine
editions of four works: `blocks_per_page` had to sit above 2 and **fails on all
nine**, reaching 1.86 at best and 0.59 at worst, which is most pages returning
nothing at all. The vertical/horizontal split does not reliably follow the
edition either — one volume comes back backwards on **both** sides at once, its
Japanese raw reading 71% horizontal while its English edition reads 66%
vertical.

**Now measured against hand-drawn ground truth rather than by proxy, and the
figure is 0.4%.** The dev set of `docs/superpowers/specs/…-l2-ground-truth-instrument-design.md`
was labelled: 45 pages across the same nine editions, **480 boxes**, one per
text run. `taiyaku score` over all of it:

| outcome | boxes |
| --- | --- |
| 1:1 | **2** |
| merged | 47 |
| split | 0 |
| fragment | 0 |
| missed | **429** |
| spurious | 3, or 0.07 per page |

Two of 478 scored boxes, both on `oshinoko-v01/en-mono`. Every figure above
this paragraph was a proxy for recall — `blocks_per_page`, the v/h split, block
composition — and each was right about the direction; this is the quantity
itself.

**The shape of the failure is the useful part, and it is what the five outcomes
were for.** `split` and `fragment` are both **zero**: the detector never
over-segments, so it is not finding text and cutting it wrongly. It is not
finding it. `spurious` is 3 across 45 pages, so it barely proposes anything at
all — and the 47 merged are what happens when the little it does propose lands
across several boxes at once. Counted directly on `gotoubun-v01/raw-mono`'s
five labelled pages: **45 truth boxes against 3 proposals**, with pages 9, 48
and 165 getting none whatever. Over that whole volume it emits 148 regions on
195 pages, **135 of which get nothing**, and the median region it does emit
covers **8.6% of the page** where a dialogue balloon covers about 1%.

**This is a floor, not a tuning problem.** With spurious at 0.07 per page there
is no precision traded against recall to recover, and `COVERAGE` — provisional
at 0.5, and still unmeasured — cannot be the explanation: no threshold takes 3
proposals to 45. Any candidate that proposes roughly one region per text run
improves both promotion terms at once, which makes this a cheap number to beat
and an honest one to be measured against.

**The L2b candidate has now been measured against that floor and is promoted:
48 of 478, at 6.73 spurious per page.** It is three changes to `detect_page`,
not one — a component-size band before the smear, a two-axis smear unioned and
deduped instead of one axis chosen per page, and `MIN_INK` counting ink pixels
instead of summed stroke bounding boxes — and its band was fixed by a sweep over
the same 45 pages rather than chosen. Both figures are sums over all nine
editions:

| outcome | incumbent | candidate |
| --- | ---: | ---: |
| 1:1 | 2 | **48** |
| merged | 47 | 73 |
| split | 0 | 152 |
| fragment | 0 | 81 |
| missed | **429** | 124 |
| spurious | 3 (0.07/page) | 303 (**6.73**/page) |

**The verdict, stated against the rule fixed on 2026-08-11 rather than against
the outcome:** a candidate is promoted only if it increases 1:1 *and* keeps
spurious-per-page at or under the dev set's own 10.7 truth boxes per page, ties
to the incumbent. Term 1 is 48 against 2. Term 2 is 6.73 against 10.7. **Both
pass, so it is promoted** — the first candidate on this problem that has been,
after two refusals under rules fixed the same way.

**The band was swept, and the losing rows are the deliverable as much as the
winner.** Eight values of `BAND`'s `lo` were scored over all 45 pages;
spurious-per-page falls monotonically from 294.40 to 0.42 across them while 1:1
peaks at `lo = 32` and falls away. `lo = 32` finds the most 1:1 matches of any
row — **57, nine more than the promoted band** — and is refused at 12.89
spurious per page. The whole table is in `notes.md`, "Where the band goes,
measured at eight bands over 478 hand-drawn boxes", and `band_sweep.rs`
reproduces it.

**What is promoted is still a detector that does not work, and the failure has
inverted rather than closed.** The largest column is no longer `missed` (124)
but `split` (152), with 81 fragments beside it: 233 of 478 boxes now carry a
prediction of the wrong shape, where the incumbent carried none at all. 48 of
478 is 10%, against phase 1's geometric ceiling of about 354 of 478 for any
smear-based detector. **Per kind, every dialogue hit is on an English edition** —
43 of 389 dialogue boxes overall, and 0 of 38, 0 of 40, 0 of 45 and 0 of 44 on
the four Japanese raw-mono editions — with 5 of 63 sound effects and nothing at
all among the 26 logo, credits, narration and other boxes. Vertical Japanese type
in a balloon is not being found. That per-kind split is the evidence the L2
design defers its C2 question (balloon-interior detection) to, which is why it is
recorded beside the total rather than under it.

**And `lo = 64` is not a boundary between two measured populations.** Phase 1 put
the outside population's p95 at 17‱ of page height and the inside population's
median at 8‱; the promoted cut is nearly four times the former and discards most
of what a person's own boxes contain. It wins anyway, because what it removes is
what welds — a result about the smear rather than about glyph size. 8 and 16 were
both swept and both lose.

**The cause is measured rather than guessed, and it is not the one that was
predicted.** The scale every gap is built from, `stroke_scale`, is the median
longer side over every ink component on the page — and 63% to 96% of those
components are **under 5 pixels**: screentone dots, JPEG ringing, antialiasing
fragments. So the scale collapses to 1–3px on pages 685–2147px wide and the
smear gap to 4–6px. The blob still forms, but not because the gap is large:
screentone packs ink densely enough that a few pixels of tolerance already lets
`merge_along`'s transitive passes weld thousands of specks into one component
spanning most of a page, which `MAX_BLOCK_AREA` then discards. On all eight
sampled non-blank pages it discarded the largest thing on the page, covering
33.6% to 96.6% of it.

**A passing `median_area` is not evidence of health**, which is worth stating
because it looks like it. `MAX_BLOCK_AREA` discards anything above 0.15 *before*
the median is taken, so that check is satisfied by the backstop rather than by
the detector finding text.

**Computing the scale over a population that excludes sub-glyph noise was
tried, twice, and neither attempt cleared the bar either.** L2A Task 9 fixed
an acceptance rule before measuring anything — beat `all` on both
`blocks_per_page` clearing 2 and the v/h split following the edition on both
sides of one work — then measured two candidates against it over the same
nine editions: `above-floor` (the median over components above a noise floor
derived from the page's own size distribution) and `modal` (the mode of the
size distribution rather than its median). `above-floor` is worse than `all`
on `blocks_per_page` for 7 of 9 editions, four of them dropping to zero
blocks across every sampled page; it overshoots past glyph size into art
size, because excluding the noise floor does not isolate a second, glyph-sized
population on most pages — there rarely is one — so the median of what is
left is a mix of type and everything else that is not screentone. `modal`
raises mean `blocks_per_page` from 0.84 to 1.02 but gets no edition newly
above 2, and neither candidate reaches even one work with a correct v/h split
on both sides, where `all` already reaches one (a fact this task also found:
the brief that set the bar believed nothing did). Per the rule fixed before
either candidate was measured, neither is promoted; the detector is
unchanged, and the constants' doc comments still say "a starting value, not a
measurement" because that is still true — this task measured the scale rule,
not the five thresholds built on it. See `notes.md`, "The text-block
detector, measured against four works, and why it does not work yet" and "The
scale rule, tried three ways, and neither candidate clears the bar".

**Nor is a passing `blocks_per_page` evidence of health, and that is the
sharper of the two.** The diagnosis above had a hole in it — the noisiest
edition sampled, `gotoubun-v01/en-mono` at 93%–99% of components under 5px, is
the *only* one of the nine to clear the design's bar, at 2.50, which the
screentone-welding account predicts should fail worst of all. Rather than start
a redesign on a mechanism that could not explain its own outlier, the same
harness was asked what that edition's surviving blocks are made of rather than
how many there are. **The median one holds 1,877 components whose median longer
side is 2 pixels**, on a 2147×3056 page — the same material as the page around
it, which is 97.6% sub-5px. 23 of the 25 are tagged with the wrong axis.
Meanwhile the edition whose blocks are made of markedly coarser components —
`oshinoko-v01/en-mono`, 12 of 14, correctly horizontal — scores 1.40 and
**fails** the bar. The two rank opposite ways under the count and under the
composition, so `blocks_per_page` cannot be the acceptance rule for whatever
replaces the detector.

**Ten of those blocks were then cropped and looked at, which corrected the
account of them.** Statistics alone had them down as fields of screentone;
they are **whole panels** — art, tone and the dialogue together — so the
detector on this edition is finding panels rather than text blocks. That
explains every figure at once: a panel sits comfortably under
`MAX_BLOCK_AREA`, so 2.50 `blocks_per_page` is roughly panels per page; a
panel has no reading direction, so the axis is wrong; a panel's components are
its screentone, so the median is 2px. One block of the six examined there is a
clean caption box. The same page also shows the opposite failure — seven
"blocks" that are vertical slices of one word — so one edition at one gap both
over-merges panels and under-merges lettering. `oshinoko-v01/en-mono`'s, by
contrast, were seen to be a title logo, a credits line and a caption box, plus
one panel. See `notes.md` for the table and for what ten blocks of 76 does and
does not establish.

**Why the noisiest edition scores best is the mechanism correction.** Not that
it escapes the welding — that it welds at a proportionally finer gap. The
noise-collapsed 2px scale gives the same ~4px gap everywhere, but on a page 3.1×
wider than `gotoubun-v01/raw-mono`'s that is **1.9‰ of page width against
5.8‰–14.6‰**. Where the coarser relative gap welds a page into one over-large
blob and `MAX_BLOCK_AREA` throws the whole page away, the finer one chops the
same screentone field into several *mid-sized* fragments, each under 0.15 of
the page — and `MAX_BLOCK_AREA` is the only filter with an opinion about size.
The edition scores best because its noise is cut into pieces the right size to
pass.

**Two further things about the mechanism were wrong and are now measured.**
First, the page-spanning blob is not always built by the smear: on **47 of the
89** sampled pages where `MAX_BLOCK_AREA` discarded something, the thing
discarded was already one connected component before `merge_along` ran at all —
7 of 10 on `gotoubun-v01/en-mono`, equal to the printed decimal. Denoising the
population that feeds `stroke_scale` cannot reach those pages; only changing
what `components` returns can, which bears directly on the route out of here.
Second, **`MIN_INK` has never rejected a block** — 0 blocks on 0 of 90 pages,
against `MAX_BLOCK_AREA`'s 166 on 89 — because it sums overlapping bounding
boxes, so 61 of the 76 surviving blocks score *above* 1.0 against a floor of
0.05. `MAX_BLOCK_AREA` is not the backstop the design calls it; it is doing
essentially all the filtering. That is O34. See `notes.md`, "The edition that
clears the bar is the one finding no text".

**Mutation testing found 346 survivors across the workspace. All five crates
are now closed, and every survivor that remains is either an equivalent mutant
carrying its argument or deliberately deferred.** Of the 346 — `taiyaku-core`
47, `taiyaku-cli` 217, `taiyaku-serve` 52, `taiyaku-read` 17, `taiyaku-wasm` 13
— read, wasm and serve are at zero, core is at three equivalent mutants and cli
at three more, with its remaining 44 deferred until L2's detector is redesigned
for the reason given below. `env -u TAIYAKU_CORPUS
cargo mutants --gitignore=true --test-tool=nextest` ran 1,660 mutants in 43
minutes on 2026-08-10: 1,149 caught, 346 missed, 136 unviable, 29 timed out —
75.4% of 1,524 viable mutants caught. `taiyaku-core`'s 47 were concentrated in
five files — `solve.rs` 26, `sidecar.rs` 11, `credits.rs` 7, `slot.rs` 2,
`lib.rs` 1 — and re-measuring the crate after the work gives **458 mutants,
411 caught, 3 missed, 38 unviable, 6 timed out: 97.9% of viable caught against
87.7% before.** All three remaining survivors are equivalent mutants carrying
their arguments, and both figures that moved outside the survivor count have
reasons rather than noise: two mutants disappeared because `percentile`'s
clamp turned out to be unreachable and was deleted, and one more became a
timeout because a new traceback test made a mutant hang rather than lie.
`notes.md`, "The mutation baseline, and the minus sign nothing was testing"
and "The 47, killed — and what the fixtures got wrong", has the full record,
including the gap penalty in `solve::solve`'s first column that could be turned
into a bonus
with all 526 CI-visible tests still passing, and the six planned fixtures that
passed while discriminating nothing. **`taiyaku-read` and `taiyaku-wasm` are now
closed too — 30 survivors to 0**, re-measured at 140 mutants, 110 caught, 22
unviable, 8 timed out. Their gaps were four, and each was a whole surface rather
than a scattering: the wasm shell's five value-forwarding methods, which
`binding`'s tests sit one layer below and cannot see; `tokenize`'s decimal-point
lookahead, in a file that carried one test; `MAX_DECODE_ALLOC`, a number nothing
asserted; and four odds in `pages.rs`. One survivor was not killed but
*deleted* — `unsupported_container`'s `len() > 262` was one past what the slice
it guards needs, and rewriting it as `bytes.get(257..262)` removed the
arithmetic along with three already-caught mutants at the same site. The eight
timeouts are all non-terminating `tokenize` loops and are O39's disposition
rather than untested behaviour. `notes.md`, "The other 30, and the four
surfaces they were hiding in", is the record. **`taiyaku-serve` is now closed too — 51
survivors to 0**, re-measured at 307 mutants, 272 caught, 31 unviable, 4 timed
out and none missed. Its gaps were concentrated rather than scattered: **27 of the 51 were in
`UreqTransport`**, the one component in the crate that talks to the outside
world — and the one every other test avoids by design, since `Transport` exists
precisely so the suite does not need the NAS. Nothing offline had ever driven
it, so `post_json` and `get_bytes` could each return a constant. They are now
tested against a hand-rolled `TcpListener` on a loopback port. Three more
survivors were decisions buried inside `eprintln!` arms — the warn-once latch in
`reqlog`, the "skipping N works" arithmetic in `resolve_all_works` — which no
test of either component's *output* could see; each is now a named function with
a return value. And `serve` itself could be replaced by `Ok(())`, starting no
server at all, with every other test green. `notes.md`, "The 51, and the client
that talked to nobody", is the record, including the comment in `get_page` that
justified a load-bearing filter with the condition backwards. A **fifty-second**
survivor appeared only while rebasing, on a mutant the earlier runs had caught
twice: the test guarding it is timing dependent and its own comment claimed it
was not. `notes.md`, "The mutant that was caught twice and missed once", is that
record. `taiyaku-cli` re-measures at **752 mutants,
217 missed**, 478 caught, 45 unviable, 12 timed out — the recorded figure
exactly. They fall in nine files, and the distribution is the useful part:
`main.rs` 70, `fingerprint.rs` 39, `detect.rs` 37, `report.rs` 30,
`inventory.rs` 19, `regions.rs` 7, `duplicates.rs` 7, `credits.rs` 5,
`cache.rs` 3. **`report.rs` is closed — 30 to 0**, verified at 40 mutants, 37
caught, 3 unviable; its gaps were `verdict`'s two margin thresholds, every
operator in the gauge arithmetic and the header's runs-per-hundred rate, and
`prepare`, which could return two empty vectors. Confirmed at crate scale as
well as in isolation: re-running the whole crate gives **752 mutants, 187
missed, 508 caught**, 45 unviable, 12 timed out — exactly thirty moved from
missed to caught, and nothing else shifted. `notes.md`, "The verdict bands, the
gauge, and two pages that hashed alike", is the record.
**`detect.rs`'s 37 and `regions.rs`'s 7 are deliberately not being closed**:
O31 to O34 record that the detector is measuring the wrong population and wants
a redesign, and pinning its present behaviour with 44 tests would cement what
those items exist to change.

**Five of the remaining six files are now closed — 73 survivors to 3**:
`fingerprint.rs` 39 to 1, `inventory.rs` 19 to 0, `duplicates.rs` 7 to 2,
`credits.rs` 5 to 0, `cache.rs` 3 to 0. All three survivors are equivalent
mutants and each carries its argument in `notes.md`: `flat_border`'s
crossed-scan guard, which the quarter-per-side caps make unreachable at every
page size from 8 pixels to 100,000, and two in `contradictions`, where a
singleton cluster has no pairs to examine and no producer of `Pair` ever builds
one judging a copy against itself.

Two shapes account for most of the 73, and neither is "untested code". **A
bound no fixture reached**: sixteen were the four `MAX_SIDE` caps in
`flat_border`, free because every letterbox fixture pads narrower than a
quarter, which is what a real letterbox does. **A decision inside a print
statement**: eleven were in `inventory`'s table loop, counted in a loop whose
only output is `eprintln!` — each now a named function returning a value, the
pattern `#41` established in `serve`. Three fixtures that could not fail turned
up on the way, including a ranking test whose tied clusters were listed in the
order its own assertion wanted, against a stable sort. `notes.md`, "The quarter
that never bound, and a constant no test could see" and "The loop that printed,
the test that copied it, and a fixture in the order of its own answer", are the
record.

**And `main.rs` is closed too — 70 to 0**, which finishes the 143. Its seventy
sat in five command handlers, almost none of them reachable from a unit test as
the code stood: `topology` runs against real archives, `align`, `verify` and
`lookup` print, `serve` starts a server. Three things carried it.
**Twenty-four were a single loop** — `topology`'s four per-page accumulators,
four running totals, verdict comparison and self-pair skip, all inside a
doubly-nested `for` whose only output is `println!`, now `ordered_pairs`,
`Divergence` and `PairReport`. **Fourteen died by deletion rather than by
test**: `Sidecar::edge_between` was already in `taiyaku-core`, tested and
`pub(crate)`, while `verify` and `lookup` each carried their own copy of the
two-way comparison — three copies of one predicate, and only the tested one
survived the change. **And `main` itself is only killable from outside the
process**, so `crates/taiyaku-cli/tests/cli.rs` spawns the binary and asserts
the two contracts `scripts/` depends on: a refusal exits 1 with `taiyaku: `
leading stderr and nothing on stdout, and no arguments prints the usage and
exits 0.

A fourth fixture that could not fail turned up here, written in this session
and caught only by re-measuring: `route_verdict`'s test compared `None` to
`None` throughout, because it named an edition the fixture does not define.
`notes.md`, "main.rs: 70 to 0, and fourteen that died by deletion", is the
record.

Confirmed at crate scale as well as file by file: re-running the whole crate
gives **785 mutants, 47 missed, 669 caught**, 57 unviable, 12 timed out,
against 752 / 187 / 508 / 45 / 12 before. The 47 are exactly `detect.rs`'s 37,
`regions.rs`'s 7 and the three equivalent mutants, and the 33 mutants the crate
gained are the extracted functions, every one of them caught. That leaves
**`detect.rs` and `regions.rs`** as the whole of what is measured and open, and
both are waiting on L2.

### Open items

Thirty-nine, each found by a whole-branch review, carried as a deferral,
turned up by reading a design against the code, turned up by running a
command against its own usage text, turned up by reviewing what the fix
before it left behind, turned up by measuring what a new feature or a fix
produced against the corpus, turned up by designing the fix for another one,
turned up by checking a diagnosis against the one case it did not explain,
turned up by simulating the reader from every position it can hold, turned
up by reading a file closely enough to write tests for it, or turned up while
reading the code around a line a tool had pointed at, and
each judged not worth blocking on at the time. They
are tracked here rather than left in a scratch file so that closing one is a
visible act rather than a thing someone remembers. **Thirty-one are now closed
— O1, O2, O3, O4, O5, O7, O8, O10, O12, O13, O14, O15, O16, O17, O18, O19, O20,
O21, O22, O25, O26, O27, O28, O29, O30, O34, O35, O36, O37, O38 and O39 —
leaving eight.**

Three of those closed the same day they opened, which the "judged not worth
blocking on" framing above does not fit: O35, O36 and O37 were found by the
coverage branch and fixed on it. They are recorded here anyway rather than only
in that branch's entry, because this table is where a reader looks for what was
wrong and whether it still is, and a defect that never appears here because it
was fixed quickly is a defect nobody can later find.
Closed rows stay in the table rather than being deleted, because a list that
only ever grows says nothing about whether anything gets fixed. The evidence for
each is in the detail below; the table is what a reader scans.

| ID | Open | What | Where | Opened by |
| --- | :---: | ---- | ----- | --------- |
| O1 | [x] | Page responses carry no cache headers — *closed by removing the round trip rather than by counting them: 0 page requests reached the server across a ten-slot re-read* | `routes.rs::get_page` | M2 review |
| O2 | [x] | Progress is written non-atomically | `progress.rs::store` | M2 review |
| O3 | [x] | `PUT /progress` does not range-check the slot | `routes.rs::put_progress` | M2 review |
| O4 | [x] | Two sidecars claiming one work id is untested | `works.rs::load_works` | M2 review |
| O5 | [x] | A non-numeric page index answers axum's default 400 | `routes.rs` | M2 review |
| O6 | [ ] | A cover from page 0 can be a 430 MB decode — *measured: 96–221 ms on real material* | `cover.rs` | M3 review |
| O7 | [x] | The request log grows in memory for nothing | `reqlog.rs::Log` | M3 review |
| O8 | [x] | Stump resolution blocks the listener | `lib.rs::serve` | M3 review |
| O9 | [ ] | No single-flight guard on the request path — *measured: nothing to coalesce* | `composite.rs::StumpSide` | M3 review |
| O10 | [x] | `MAX_DECODE_ALLOC` is duplicated across crates | `cover.rs`, `taiyaku-cli/src/fingerprint.rs` | M3 review |
| O11 | [ ] | Neither on-disk cache notices a change on the far side — *escape hatch added; covers measured at 165 KB, filmstrip tiles at 1.12 MiB per 195-page edition* | `stump/cache.rs`, `cover.rs` | M3 review |
| O12 | [x] | `spawn_blocking` covers three of five blocking handlers | `routes.rs::get_progress`, `::put_progress` | M1/M2 deferral |
| O13 | [x] | A hand-corrected run does not survive a re-solve | `sidecar.rs::merge_spoke` | M4 planning |
| O14 | [x] | The binding rebuilds the route on every resolve | `binding.rs::Binding::resolve` | M4 whole-branch review |
| O15 | [x] | A flag typed with no usable value was taken as absent, in silence | `main.rs::option_value` | duplicate-resolution review |
| O16 | [x] | A symlink cycle truncates the inventory walk — *silently, not by crashing* | `inventory.rs::totals` | duplicate-resolution review |
| O17 | [x] | The content-hash confirmation re-reads a copy once per candidate | `duplicates.rs::representatives` | duplicate-resolution review |
| O18 | [x] | The `--against` flagging rule had no corpus witness | `duplicates.rs::resolve_with` | duplicate-resolution review |
| O19 | [x] | `align` without `--split-spreads` ignored the whole sidecar-writing half of its flags | `main.rs::align` | O15 scoping |
| O20 | [x] | Six more `align` flags were ignored where they could not take effect | `main.rs::align` | O19 fix |
| O21 | [x] | A misspelled flag was accepted in silence by every command | `main.rs::reject_unknown_flags` | O20 review |
| O22 | [x] | `verify` ignored `--cache` and `--ltr` with no `--sidecar` to apply them to | `main.rs::verify` | O20 review |
| O23 | [ ] | Two hand corrections make a sound alignment report `fragmented` — *measured: avg run 64.33 → 32.00 → 21.22 on Go-Toubun v01, against a 27.0 threshold* | `sidecar.rs::rewrite`, `solve.rs::verdict` | M5d |
| O24 | [ ] | A solver-written direct edge to a third edition survives that edition being replaced | `sidecar.rs::merge_spoke` | M5d whole-branch review |
| O25 | [x] | The settings panel and the whole chrome are unreachable while showing a gap | `App.tsx::App`, the `at` narrowing | M5c browser pass |
| O26 | [x] | `align --write` truncates the sidecar before writing it | `main.rs::write_sidecar` | credit-pages whole-branch review |
| O27 | [x] | Two refs are held and read by nothing | `App.tsx::App`, the ref block above `viewportWidth` | O25 design |
| O28 | [x] | A forward arrow on the last group's second slot moves backward | `pairing.ts::stepToGroup` | credits-panel arrow simulation |
| O29 | [x] | A failed `--write` names a temp path the caller never typed | `atomic.rs::write` | O26's corpus drive |
| O30 | [x] | A relative `TAIYAKU_CORPUS` skipped every standing check and reported `ok` | `corpus.rs::corpus`, `corpus.rs::corpus_root` | O26's corpus drive |
| O31 | [ ] | The text-block axis is decided per page, where the design says per cluster — so a page cannot report vertical dialogue beside a horizontal SFX, and the survey's `v`/`h` figures are per-page decisions counted per block — *all three of those consequences are addressed by L2b Task 5 (`8531d05`): `axis_of` is `#[cfg(test)]`, `detect_page_with` unions both axes and dedupes with `uncontained`, and `region_experiment::summarize` counts per surviving block. **Open on what replaced them**: where both smears produce an identical box the axis reported is the dedupe tie-break's default rather than a measurement — 13 of 69 blocks changed label on an unchanged box over the 90-page sample — so `Region::axis` is still not "what the clustering knows", which is what this item tracks* | `detect.rs::detect_page_with`, `detect.rs::uncontained`'s tie-break | L2a whole-branch review |
| O32 | [ ] | The `median_area` disqualifier cannot fire — `MAX_BLOCK_AREA` clamps every region under it before the median is taken | `detect.rs::detect_page_with`, `regions.rs::median_area` | L2a whole-branch review |
| O33 | [ ] | Two of the five detection constants reject nothing in any test, so a computation bug in either would ship unseen — *the gap is in the tests and stays open; against the corpus `MAX_BLOCK_AREA` fires on 89 of 90 pages and `MIN_BLOCK_STROKE_SPAN` on 1* | `detect.rs`, the `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` filters | L2a whole-branch review |
| O34 | [x] | `MIN_INK` cannot reject anything on real material — it sums overlapping stroke bounding boxes, so 61 of 76 surviving blocks score above 1.0 against a floor of 0.05, and it rejected 0 blocks on 0 of 90 sampled pages — *closed by `aa0b479` (L2b Task 6) taking the first of the two routes this item named: it counts ink pixels at the page's own Otsu threshold, so the quantity is bounded by the block's own area by construction rather than by the material, and the ratio across the same sample fell from 0.38–1.89 to 0.09–0.56. It now rejects: 16 spurious blocks over the nine labelled editions, spurious-per-page 55.31 → 54.96. **Band-free it still rejects nothing on the 90 sampled pages** — the filter is live in the shipped, banded detector and not in the population `region_experiment.rs` samples* | `detect.rs::detect_page_with`, the `MIN_INK` filter and `ink_in` | the survivor-composition measurement |
| O35 | [x] | A `PUT` response with no `ETag` turns every later note write into a silent no-op — the same "write without a validator" hole the 412 path was fixed for, left open on the response path — *closed on the branch that found it (178f3e5): the guard became `writeGate` with three arms — `wait` (no document, the only silent one), `refuse` (document but no validator, carrying a notice), `write` — after the correct-behaviour test was watched failing against a faithful port* | `api.ts::putNotes`, `App.tsx::writeNotes`'s `notesEtag === undefined` guard | coverage-gate reading |
| O36 | [x] | The two write debounces are never cancelled on unmount, so an unmounted `App` can still `PUT` up to 700 ms later — *a two-line fix, not an impossibility: `useFlushOnHide`'s cleanup already exists and `debounce` already exposes `cancel()`* — *closed on the branch that found it (4303dee): `useFlushOnHide`'s cleanup now flushes both, decided per call site: cancelling `writeProgress` drops the last page turn, cancelling `saveNotes` destroys up to 700 ms of prose that exists nowhere else* | `App.tsx`'s `writeProgress`/`saveNotes` handles, `hooks.ts::useFlushOnHide` | coverage-gate reading |
| O37 | [x] | `reviewNotes`'s doc comment describes an arm of `toggleReview` no key can reach — it claims `a` closes the editor mid-edit, which `keyplan.ts` refuses; the code is right and the comment is wrong — *closed on the branch that found it (f4047b7): corrected, and the same wrong claim in `notes.ts` with it in `7cab6ec`* | `App.tsx::reviewNotes`'s doc comment | coverage-gate reading |
| O38 | [x] | `unpaired_runs` counts a gap run's own `a` span as "mentioned", so asking the `from` side of an edge which of its pages have no counterpart answers `[]` for pages that demonstrably have none — reachable today as `taiyaku credits <hub member>... --sidecar F --against <spoke>`, which prints zero candidates — *closed by `run.b.map(|_| run.a)`, with a corpus drive that found it answering `[]` on **all twelve** volumes — 26 gap runs over 36 raw pages — and hiding a real `credits` verdict on kobayashi-v01's last page* | `credits.rs::unpaired_runs`, the `true => Some(run.a)` arm | the Task 7 mutation review, by probe |
| O39 | [x] | Nothing bounds a test's runtime, so a regression that stops a loop terminating hangs the forge instead of failing it — there is no `.config/nextest.toml`, no `timeout-minutes` in `ci.yml`, and CI does not run nextest at all but `cargo llvm-cov` over `cargo test` — *closed by commits (`dadd6a2`, `bce5bf3`), PR #47: a 60 s per-test kill via `.config/nextest.toml` under `cargo llvm-cov nextest`, and `timeout-minutes: 10` on every job — the per-test bound was triggered on the forge itself (a deliberate hang failed the `test` job at 153 s, its exact revert passed at 117 s); the job-level bound was not, both runs finishing far under ten minutes* | `.config/nextest.toml`; `.forgejo/workflows/ci.yml`'s four `timeout-minutes` keys | the Tasks 2-6 mutation review |

Sites are named by symbol rather than line, because a line number in a document
is wrong the first time someone edits the file above it.

**`serve` was a first cut, and five things about it were known thin** — O1 to
O5, each found by the whole-branch review. All five are now closed. O1 was the
last and the longest-lived: it closed once, on a browser-reader `304` count
that looked like the second measurement the first one (against Panels) had
been waiting for — and reopened on review, when the count turned out to be
measuring this task's own traffic as much as a reader's. It closed for good on
the *other* of the two exits its own entry named, which needed no session log
at all. O5 was nearly deferred alongside it on the strength of a wrong
estimate — "a custom rejection handler touches routing for every route" —
which was checked rather than repeated and turned out to be two handlers and a
local parse:

- **O1 — Page responses carry no cache headers. Closed.** *Everything from
  here to "What would close it" is the record as it stood while the item was
  open, kept rather than rewritten, because the reopening is the part worth
  reading. The close is at the end.* The bytes for a
  given work, edition and index never change, and the sidecar already
  records a `sha256` per member, so an `ETag` (shipped the task before this
  one — `routes.rs::locate_member`/`page_etag`) was a line of code once the
  evidence question was fixed (see `notes.md`, "An ETag would have saved
  nothing, and the obvious way to check that is wrong"). The validator
  genuinely shipped and genuinely produces `304`s — each hit turns 150–850 KB
  into roughly 300 bytes, and that saving is real and stays in the record.
  What did not survive review was closing the item on the strength of it: the
  measurement behind the close overstated what it had shown, and the owner
  reopened O1, annotated with the number rather than hidden behind it.
  **Two different real behaviours showed up in the literal scenario, and
  only one of them is about the `ETag`.** Twenty-four `Space` presses fired
  100ms apart against one fixed slot, from a fresh page load — the literal
  scenario asked for — produced **zero** further `/pages/` network activity
  for either edition's URL after each was first displayed: not a `304`,
  nothing at all. A `PerformanceObserver` watching `resource` entries live
  (not a buffered query taken after the fact, which undercounts) recorded
  four requests across the whole run, all prefetch neighbours, none for the
  two centre URLs. A source the browser decoded a moment ago and is asked to
  redisplay does not go back to the network at all — its own short-lived
  in-memory image cache repaints directly. That is cheaper than a `304`, not
  a failure of one, but it does mean the exact swap-on-one-slot scenario
  produces no `304`s to count, for a reason that has nothing to do with the
  `ETag` — see `notes.md` for the full trace.
  **The 17-of-39 figure is real; what it was evidence of was not.** Ten
  slots read forward, swapping to the second edition and back at each one,
  then *the same ten slots read backward*, with the same swap at each,
  produced 39 `/pages/` requests: 22 full `200`s, 17 `304`-sized responses
  (classified by `transferSize`: a full page runs 150–850 KB, a
  headers-only revalidation about 300 bytes — a different thing from the
  literal scenario's zero-byte, zero-request repaint above; `transferSize`
  is what tells the two apart). That session is a revisit generator, not
  reading — nobody re-reads the ten pages they just read, backwards,
  immediately — and twelve of the seventeen `304`s are exactly that second
  pass: all twelve backward-pass requests, every one of them a genuine
  revisit of a slot the reader had left and come back to.
  **The other five are not reader traffic at all.** The forward pass alone
  produced 22 full `200`s and 5 of its own `304`s — self-inflicted, not
  genuine. The prefetch window warms a neighbouring slot into a throwaway
  `Image()` (the same default builder `decode.ts` documents,
  `() => new Image()`, garbage once its `decode()` promise settles) and
  requests it before the reader arrives; when the reader's own swap reaches
  that slot moments later, the decode gate builds a *second* `Image()` for
  the same URL and requests it again. The browser answers the second one
  with a `304` because the first put the `ETag` in its cache — but neither
  request would exist without this task's own prefetch running, so against
  a no-prefetch baseline the validator saved nothing on those five: it
  neutralised traffic the reader introduced, not traffic a reader brought to
  it. **The honest split is twelve genuine reader revisits and five
  self-inflicted duplicates**, out of one scripted session — one browser,
  one corpus, one machine, no repetition — against which O1 had already been
  deferred twice, most recently on 177 requests from a real recorded client
  session (Panels). The close named its own reopening bar as "a real session
  log … over genuine reading, the same standard Panels' own measurement was
  held to" and then closed on a synthetic script that met neither half of
  that standard — which is the finding this review turned up.
  **The instrumentation the count rests on has not itself been checked,
  beyond one indirect spot-check.** All seventeen were classified by
  `transferSize` alone. The one comparison against the wire that exists
  anywhere in this work was run as an isolated repeat — a different request,
  in a different run, from the 39-request session above — where the
  CDP-level network log confirmed the literal wire text `304 Not Modified`
  for a response Resource Timing's own `responseStatus` field called `200`
  (which normalises a validated hit to the status the *page* effectively
  got, not the status the *wire* carried, and is why `responseStatus` was
  abandoned as a classifier here — a genuinely useful trap to have on
  record). Sixteen of the seventeen — everything but that one indirect
  check — rest on `transferSize` and nothing else.
  **Also verified directly, not assumed:** a `200` carries neither
  `cache-control` nor `last-modified` — `fetch(url, {cache: 'no-store'})`
  against a never-before-requested page returned exactly `{connection,
  content-length, content-type, date, etag, vary}`. Both headers are still
  absent, unchanged by this task, so heuristic freshness is still zero and
  every reuse, genuine or self-inflicted, still asks the network before
  anything is served — never silently from disk, but never for free either.
  **That is the part that keeps O1 open on its own terms, not only on
  sample size.** O1's rationale was that the design's "one fetch, one cache
  entry" argument for serving whole spreads assumes the client caches, and
  nothing yet helps it — a validator that only shrinks the response doesn't
  change that. Prefetch exists to hide a round trip before the reader
  notices it, and a round trip that still has to happen, just for fewer
  bytes, is a round trip the reader still waits on: on a slow connection
  this saves body bytes and no latency.
  **What would close it:** a real session log, from genuine reading rather
  than a script built to force revisits, showing conditional-request reuse
  net of prefetch-induced duplicates — the same real-recorded-session
  standard, not a synthetic one, that already answered the equivalent
  question for Panels at zero. `scripts/request-summary.sh` already reports
  the conditional-request count; what is missing is a log from an actual
  reading session to feed it. Landing `Cache-Control`/`Last-Modified` so a
  genuine repeat costs no round trip at all, rather than a cheaper one,
  would close the item outright, independent of any session log.
  **It closed on that second exit, and taking it made the session log
  unnecessary rather than merely optional.** The item's own argument for
  reopening was that a validator shrinks a response and does not remove the
  round trip; `PAGE_CACHE_CONTROL` — `private, max-age=86400`, on
  `get_page`, `get_thumb`, the OPDS stream and the OPDS cover — removes it.
  **That freshness lifetime was later withdrawn and the round trip is back**,
  as `private, no-cache`: a work id is the sidecar's filename stem and every
  sidecar in the corpus is `taiyaku.json`, so one URL addresses a different
  volume's page depending only on which sidecar `serve` was started with.
  Measured on two of them — the same URL, SHA-256s `ab5da993…` and
  `17655ca0…`, under ETags the server had already derived from different
  member digests. The reasoning recorded here was sound and rested on a
  premise that was not: that the URL names the bytes. See the constant's own
  doc in `routes.rs`.
  With freshness in hand there is no reuse left to count, so the question
  "how many revisits does real reading produce?" stops gating the item: the
  answer costs nothing either way.
  **Measured from the server's side of the wire, which is the half every
  earlier attempt at this was missing.** Ten slots read forward in the
  browser reader against `corpus/gotoubun-v01`, all three passes driven
  through the real reader rather than by script: the pass that populates
  the cache, then the same ten slots read again after a full reload. The
  second read touched 14 distinct pages once prefetch is counted and sent
  **zero page requests to the server** — not zero bytes, zero requests,
  confirmed against `--log-requests`'s own JSONL rather than against a
  browser-side classifier. The only traffic in that window was ten progress
  writes, two work-list reads, and a `304` apiece for the sidecar and the
  notes. Before this change every one of those 14 would have been a
  conditional request the reader waited on.
  **`Last-Modified` was named in this item and is deliberately not shipped.**
  It would need the member file's mtime, which needs a `stat`, on the async
  worker, on every request — putting disk I/O back on the one path
  `locate_member` is written to keep clear of so a `304` can be answered
  before any work happens. It would also be the weaker of the two validators
  on material addressed by content: re-downloading a byte-identical archive
  moves the mtime and would throw away a cache entry that is still correct.
  The `ETag` answers revalidation and `Cache-Control` answers freshness;
  there is no third question left for a date.
  **`immutable` is not shipped either, and the reason is the URL rather than
  the bytes.** A page's bytes are fixed by `(member sha256, offset)` — that is
  what `page_etag` is derived from — but the URL names a work, an edition and
  an index, none of which move when an archive is replaced. `immutable` would
  tell a client never to revalidate for the entry's whole lifetime, which is
  O11's failure mode imported into the browser. The one-day bound is chosen
  from two ends rather than measured: long enough that a work reopened the
  next morning costs no round trip, short enough that a replaced archive
  corrects itself with nobody clearing anything.
  **One defect was introduced closing this and caught by driving it, not by
  the tests written for it.** `not_modified` is shared by four handlers, and
  the first version of the change put the page constant inside it — so
  `get_sidecar` and `get_notes`, which share it, began telling clients they
  could reuse a *sidecar* for a day. In the browser both documents stopped
  reaching the server at all, which is a lost correction waiting to happen:
  `PATCH /alignments` rewrites the sidecar, and a reader who made a
  correction and reloaded would have been served their own stale copy. The
  test written alongside the change did not catch it, because it asserted on
  the `200` path and the leak was on the `304` — and a client stores what a
  `304` tells it (RFC 9111 §4.3.4), so the two are worth the same. `freshness`
  is now a parameter every caller must supply, and the test checks both
  statuses on both mutable routes; reintroducing the leak fails it by name.
  See `notes.md`, "O1 — closes here, and the fix broke the sidecar on the way".
- **O2 — Progress is written non-atomically. Closed.** `fs::write` truncated
  first, so a crash mid-write left a short file and the next read answered 500
  rather than "nothing recorded" — the worse of the two wrong answers, since it
  tells the reader the server is broken rather than that it has no position to
  restore. `store` now writes a temp file and renames, the pattern the Stump
  resolution cache (`taiyaku.stump.json`) already used beside the same sidecar.
  The temp name is **unique per attempt** rather than fixed, which the cache's
  is not: progress is the one file here a person writes from two devices at
  once, and two writers sharing one temp name can interleave into publishing
  the empty file the change exists to prevent. Same reasoning and same shape as
  `cover::unique_temp_path`; the two are not shared yet. A crash cannot be
  staged in a test, so the test pins the mechanism instead — the destination is
  replaced by a rename, never opened for truncation, made observable by taking
  write permission off the existing file, and skipping with a printed line
  where that precondition does not hold (root, or a filesystem ignoring the
  mode bits) rather than reporting a green that measured nothing.
- **O3 — `PUT /progress` does not check the slot is in range. Closed.** It
  validated the edition, so a position could name a real edition and a page
  past its end. It now range-checks too, and **in slots rather than pages**:
  a position is a slot — that is why `progress.rs` records one — and an edition
  that joins spreads has more slots than pages, so checking against
  `total_pages()` would have refused the last slot of every such edition, One
  Piece v092 being spreads throughout. A test holds that distinction: 4 pages
  with page 0 a spread is 5 slots, and slot 4 must be accepted. The refusal is
  404 with the same wording shape as the edition check above it, naming the
  edition and how many slots it has.
- **O4 — Three error paths were named here; all three are now covered.**
  `PageSource` narrowed to one member during M3, so the edition
  walk that used to live in `LocalLibrary` now lives in `ServeState`. An
  unknown edition is tested there, both directly and end to end over HTTP (a
  404 for an edition a work does not have). An out-of-range page index is
  tested at the `ServeState` level and dispatched through the router on both
  the `/api/…` page route and the OPDS stream route. **Two sidecars claiming
  one work id is now tested too** — the code refused it from M2 and nothing
  exercised it, so it was correct by inspection only. The test asserts the
  message names *both* paths, since the point of refusing is that the person
  running the server cannot otherwise tell which two files collided; a second
  test loads two differently-named sidecars side by side, so the refusal
  cannot be spelled "refuse everything".
- **O5 — A non-numeric page index answered axum's default 400. Closed.** It was
  the only message this server sent that this server had not written:
  `Invalid URL: Cannot parse value at index 2 with value `abc` to a `u32``,
  which names a Rust type and a position in a path tuple, neither of which
  means anything to a client. Both routes addressed by page index — the
  `/api/…` one and the OPDS stream — now take the segment as a `String` and
  run it through one shared `page_index`, which answers ``\`abc\` is not a page
  number; pages of `raw` are numbered from 0``.
  **The status stays 400 rather than becoming 404**, deliberately: `pages/4`
  names a page an edition does not have and `pages/abc` does not name a page at
  all, and a client is entitled to tell those apart. What they now share is
  that both come from here.
  Shared rather than written twice, and pinned from both ends — Panels reaches
  pages through the OPDS route, not the `/api/…` one, so fixing only one would
  have fixed the one nothing uses. The refusal is carried by a named
  `NotAPageIndex` rather than a bare `Response`, the pattern `NoSuchWork`
  already established in the same file for the same reason: a `Response` is 128
  bytes and `Result<u32, Response>` trips `clippy::result_large_err`.

**M3 found six more, the same way** — O6 to O11, its own review, not blocking.
Three are closed. The four that remained each wanted a decision before they
wanted code, which is why they were never simply "next", and all four decisions
have now been taken by measuring rather than by arguing. **O6, O9 and O11 each
sit on a measurement and name what would reopen them** — the pattern O1
established — without any of the three closing; the artefacts were already on
disk, so nothing new had to be run. See `notes.md`, "The only concurrent
requests Panels ever issued were covers". **O8 went the other way**: the same
discipline applied to it produced a number that argued for shipping, so it is
closed. See `notes.md`, "O8, measured three ways: the third one is why it was
worth fixing".

- **O6 — A cover is generated from page 0, which on an art-book edition is
  a 430 MB decode. Open, and now bounded at both ends.** The corpus's Color
  Walk material is TIFF at that size, so the first request for such a cover is
  slow and nothing warns before or during it. The decode limit is raised to
  3 GB, mirroring `taiyaku-cli/src/fingerprint.rs`, so the request succeeds —
  but succeeding is not the same as being fast.
  **The ordinary case is now measured rather than assumed**, from the one cold
  cover grid a real client has ever produced: 95.9, 109.0 and 221.4 ms, for 54
  to 60 KB of JPEG apiece. The art-book case is about twenty times that — four
  Color Walk pages fingerprint in 8.6 s — paid once per edition and then cached
  forever.
  **What keeps it hypothetical is upstream of this crate.** A cover is
  generated for an edition named in a sidecar, and art books cannot be aligned
  at all: four pages, four pairings, every margin at or below zero and two of
  them negative. So the material that would make this decode expensive is
  material nothing should be writing a sidecar for. That is reachability, not
  impossibility — `align --write` will pair an art book against a volume and
  refuse nothing — so the premise stays one bad invocation away rather than
  excluded.
  **The same measurement settles the half of O6 that was a guess.**
  `MAX_CONCURRENT_GENERATIONS = 2` was chosen against the assumption that "a
  cold OPDS grid view issues one cover request per edition at once". It does —
  three at once, `in_flight` reaching 3, on the only cold grid ever logged. The
  guess was right and is now a number.
  **What would reopen it:** art-book linking shipping, which is the "Later"
  item that would give an art book a legitimate reason to be in a sidecar; or
  any edition whose first page is slow enough to be felt on a grid, where the
  figure to beat is 221 ms.
- **O7 — The request log accumulated in memory as well as on disk, unbounded.
  Closed.** Every entry logged to disk was also kept in a `Vec` behind a mutex
  for the life of the process, and nothing in production ever read it back:
  `Log::entries()` was called only from tests, while
  `scripts/request-summary.sh` — the actual consumer — reads the JSONL on disk.
  So the growth was real and bought nothing, on a log that exists precisely to
  be left running through a reading session. The `Vec` and `entries()` are
  gone, and `Log::new` no longer takes an `Option<PathBuf>`: a log with nowhere
  to write would now record nothing anywhere, and `serve` already expresses
  "not logging" as no `Log` at all.
  The four tests that read `entries()` read the file instead, **converted
  first and run green before the in-memory copy was removed**, so the disk
  artefact was shown to carry every fact the `Vec` did rather than assumed to.
  That also closes a weakness the old suite had and named in a doc comment:
  every other test used `Log::new(None)`, so deleting the whole file-writing
  branch would have left the suite green on the strength of one test. Every
  test here now goes through the file a person actually measures from.
- **O8 — Stump resolution happened at startup, for every member of every work,
  before the listener bound. Closed.** A summary line printed once a work
  *finished* resolving, so a work with many unresolved members was silent for
  its whole duration and a slow resolution was indistinguishable from a hung
  one. `resolve_work` now reports each member as it asks about it — its path,
  its position and how many there are. A member answered from the cache reports
  nothing, so a warm start is exactly as quiet as before: a line per member on
  every start would be noise that trains a reader to ignore it, and a test pins
  both halves.
  **The blocking itself is now gone too, and it was measured before it was
  fixed** — this item had been deferred twice on "a corpus small enough that
  nobody has felt the wait", which turned out to be right about two cases out
  of three:

  | Start | Before | After |
  | ----- | -----: | ----: |
  | Warm cache, Stump reachable | 0.10 s | 0.10 s |
  | Cold cache, Stump reachable | 3.15 s | 0.10 s |
  | Cold cache, **Stump unreachable** | **30.09 s** | **0.10 s** |

  The first two rows are why deferring was defensible: a warm start made no
  queries at all, and one nine-member work resolves in about 350 ms a member.
  **The third row is the one that changed the case.** A downed instance cost a
  full 30 s POST timeout before the listener existed — on *every* start, not
  once — to reach pages that were sitting on local disk the whole time. The
  breaker bounded it to one timeout for the whole library rather than one per
  work, so it was 30 s and not ten minutes; it was still 30 s of a server that
  could have answered everything and refused to start.
  **The fix is that resolution now runs behind the bound listener**, on a
  blocking task, with `StumpSide` holding its resolutions behind an `RwLock`
  instead of owning a finished `HashMap`. The map starts empty and fills as
  each work finishes. A member not yet resolved is indistinguishable from one
  Stump does not have — both say "read it off disk" — so the window is safe by
  construction rather than merely short.
  **Reporting is per work, not one aggregate at the end**, which is the half
  that would have been easy to miss: handing back every work's answers together
  would have moved the wait behind the listener rather than removing it, and a
  twenty-work library would still have served all twenty from disk until the
  twentieth was asked about.
  Measured after: against a deliberately unroutable address, the listener binds
  in 0.10 s and three real pages — 1.18 MB, 1.34 MB and 339 KB — are served in
  about 4 ms each **while the resolution is still hanging on its 30 s timeout**.
  Two tests, each checked by sabotage rather than by having once failed to
  compile: neutering `learn` makes the first serve disk bytes where Stump bytes
  were required, and aggregating instead of reporting per work collapses the
  second's two batches into one. What is **not** claimed is the ordering inside
  `serve` itself — that binds a socket, which no test here drives.
- **O9 — There is no single-flight guard on the request path. Open, and
  measured against.** Several requests arriving during one Stump timeout all
  wait it out independently rather than sharing the one outcome.
  **The concurrency exists; it is the wrong kind.** `notes.md` records
  "concurrent page requests: none observed, in any session", and the qualifier
  turns out to be doing real work — that claim was read here as "Panels issues
  no concurrent requests" while this item was being scoped, and it does not say
  that. Across 242 page requests in two sessions no page request was ever in
  flight beside another, but `in_flight` reaches 3, and every overlapping
  request is a **cover**. Panels fetches a work's covers together, so a
  three-edition work produces a three-deep burst on a cold cache — down the
  same `ServeState::page` → `CompositeSource::page` path pages take, and so
  squarely inside what O9 describes.
  **It is still not a burst single-flight can merge.** A guard deduplicates
  concurrent requests *for the same key*; this burst is one request per
  edition, three different members resolving to three different Stump books.
  There is nothing to coalesce, so the guard would add a map, a lock and a wait
  and merge none of the only requests that have ever overlapped here. The
  repeat case is already covered by the cooldown latch, which sends every
  request after the first failure straight to disk; what stays unguarded is
  requests already in flight when that first failure returns, and this log
  bounds that at three, all distinct.
  **What would reopen it:** a client issuing concurrent requests for one key.
  The browser reader is the candidate — prefetch colliding with a reader
  jumping onto the page being prefetched, say. Not the filmstrip: this
  paragraph used to name it as the trigger for O14 too, and both guesses were
  wrong the same way. The filmstrip resolves nothing (`nearestPaired` was
  O14's real trigger, not the filmstrip — see its own entry) and its tile
  requests were checked directly once it shipped and do not reopen this
  either — see "The filmstrip's tiles do not reopen O9", below.
- **O10 — `MAX_DECODE_ALLOC` was duplicated. Closed.** Between
  `taiyaku-cli/src/fingerprint.rs` and `taiyaku-serve/src/cover.rs`, across a
  crate boundary with no dependency edge between them. Each copy said so in a
  comment, and a comment asking whoever changes one to keep the other in step
  is the arrangement that drifts. It now lives in `taiyaku-read`, which both
  already depend on — it is the crate they share — and whose pages are the
  bytes both of them decode, so the ceiling on that decode sits alongside them.
  Not `taiyaku-core`, which compiles to WebAssembly and describes the format; a
  native decoder's memory ceiling is neither. No new test: the duplication is
  removed rather than checked, so the compiler holds what a test would have
  had to assert, and the existing decode tests on both sides cover the move.
- **O11 — Two on-disk caches never notice a change on the far side, and this
  was documented nowhere until now.** `taiyaku.stump.json`, written beside each
  work's sidecar (same directory, same stem), records the Stump book each
  member resolved to — and, deliberately, records the members Stump does not
  have too, so a miss costs one query once rather than one per page turn. That
  is also its weakness, named as Risk 5 in the design and left unaddressed:
  nothing watches for the library changing on Stump's side, so a book added to
  Stump after a work's first resolution stays invisible to taiyaku until
  someone knows to delete `taiyaku.stump.json`, which forces every member of
  that work to be re-resolved on the next start. **That last sentence is now a
  flag rather than a piece of folklore:** `taiyaku serve --refresh-stump`
  forgets every handed work's resolutions before resolving them again, which is
  precisely what deleting the file by hand did, and it says so rather than
  doing nothing when no `--stump-url` was given. The tradeoff itself stands —
  a remembered miss still costs one query once instead of one per page turn,
  and nothing watches Stump for changes — but acting on it no longer requires
  having read this paragraph. The generated-cover cache has the same
  shape without the excuse of a design tradeoff behind it: covers are
  unbounded and never pruned — an archive that is replaced gets a new
  cover under its new hash, but the old file is never reclaimed.
  **That second half now has a size, and it is 165 KB.** Three files, one per
  edition of the one work ever served, at 54 to 60 KB each. Growth needs an
  archive to be *replaced*, not a page to be read — the key is the member's
  `sha256`, so reading a work forever adds no files at all — which puts a
  hundred works at three editions apiece, every one replaced once, at about
  33 MB. Unbounded in form, negligible in size, and stated here so that the
  next person to read "never pruned" knows what it has actually cost.
  **The two caches no longer share a tree, and that is new.** They did:
  `.taiyaku-cache/covers/` was the server's, and the 55 `<sha256>.json` files
  beside it — about 1.2 MB — are the CLI's fingerprint cache, the one
  `align --cache` names and the one measured at 3.4 s → 189 ms. The command
  has to be said with the flag now: `serve` took a `--cache` of its own on
  2026-08-11 and it names the server's root, so a bare `--cache` no longer
  picks out one of the two caches this sentence exists to tell apart.
  Deleting the parent threw away both, and only one is cheap to rebuild.
  Since 2026-08-11 the server's cache lives under `$XDG_CACHE_HOME/taiyaku`
  (or `$HOME/.cache/taiyaku`), so wherever either variable is set — every
  ordinary environment — `.taiyaku-cache/` holds the fingerprint cache alone
  and clearing it can no longer discard the expensive half by accident. The
  hazard is recorded rather than deleted because a warning that vanishes
  without explanation reads as an oversight — and it has not vanished
  entirely: where neither variable is set, as under a `systemd` unit without
  `Environment=HOME` or a bare `env -i`, the server falls back to the old
  relative path and the two share a tree again. That branch is the one that
  prints, naming the directory it settled on and the `--cache` flag that pins
  it, so the exception announces itself rather than being found later as a
  pile of regenerated pages.
  **What would reopen it:** archives replaced routinely enough for 55 KB
  apiece to accumulate, or a second consumer of the server's cache directory —
  the filmstrip's thumbnail-per-page route, deferred to M5 in `cover.rs`.
  **That second consumer now exists (M5a Task 2) and has been measured, in a
  `thumbs` directory that is a sibling of `covers` rather than the same
  directory** — both under `.taiyaku-cache` when the measurement was taken,
  and both under whichever root resolves since 2026-08-11; the sibling
  relationship is what the figures below depend on, and it holds under every
  one of the four rules.
  Fetching every tile of one 195-page edition (`corpus/gotoubun-v01`'s
  `raw-mono`) produced 195 files totalling 1,177,982 bytes — about 1.12 MiB —
  individual tiles from 1,117 to 8,808 bytes. Roughly seven times the covers'
  bytes for sixty-five times the file count, which is proportionate to 195
  tiles against one cover rather than the kind of blowup "changes the
  arithmetic entirely" was written to warn about. See `notes.md`, "M5a Task 2:
  the filmstrip's tile cache, measured against the real corpus".
  **Still not closed:** the reopening condition stands, now at the tile
  cache's larger constant factor, and a library with many editions' filmstrips
  warmed at once is a case this one-edition run does not speak to.
  **A third consumer looked likely and turned out not to be one.** `taiyaku
  regions` decodes every page of an edition, which is exactly the shape of work
  the fingerprint cache exists for, and the L2a design said it would cache under
  `.taiyaku-cache/` keyed on the member's `sha256`. It does not, and takes no
  `--cache` at all: such a cache would hold the same boxes the regions document
  already holds, under the same key, in a second place that can disagree with
  the first — which is the duplication O10 was closed to remove — and `--write`
  is what makes a second run cheap. So `regions` adds no cache of its own, the
  tree it would have shared ordinarily holds the fingerprint cache alone now,
  and the flag is refused rather than accepted and ignored.

**One deferral was carried rather than found** — O12, named while planning M1
and M2, and now closed:

- **O12 — `spawn_blocking` covered three of the five handlers that do blocking
  I/O. Closed.** `get_page`, and the OPDS `stream_page` and `cover`, handed
  their work to a blocking pool; `get_progress` and `put_progress` read and
  wrote files directly on the async worker. All five do now, so the only
  handler left on the worker is `get_sidecar`, which serves the parsed
  document from memory and touches no disk at all.
  `put_progress`'s validation moved inside the boundary with its write rather
  than staying outside it: it blocks on nothing, but splitting the handler
  across the boundary would mean looking the work up twice, since a borrow
  cannot cross into the closure. A small type, `NoProgress`, carries the
  distinction `spawn_blocking` would otherwise flatten — a work, edition or
  slot that does not exist is a 404 and the client's mistake; a position file
  that will not read or write is a 500 and this server's. `get_page` needs no
  such type because both of its failures are 404s.
  **Measured, not asserted.** A FIFO at the progress path blocks a real read on
  the real code path, and on a deliberately `current_thread` runtime a read
  that runs on the worker stops the runtime dead — so a canary task never gets
  scheduled and a watchdog thread has to rescue the request, which it records.
  Before the change the test fails in 0.56 s; after it, it passes in about
  60 ms. The write path gets the same treatment and is **not** measured that
  way: `store` writes through a temp file whose name is unique per attempt, so
  there is no path a test can pre-create as a FIFO.
  The other deferral recorded alongside it was already closed: the index →
  (member, offset) walk now lives in `taiyaku_core::Edition` rather than in
  `LocalLibrary`.

**One came from reading a design against the code** — O13, turned up on
2026-08-01 while planning M4, and not blocking because nothing writes a
hand-corrected run until the correction UI exists:

- **O13 — `sidecar.md` promises a hand correction survives a re-solve, and
  `merge_spoke` did not keep one. Closed.** The format doc is explicit: `by` is
  `"solver"` or `"hand"`, and "a hand-corrected run must survive a re-solve;
  this is the field that makes the correction UI more than a viewer." What
  `merge_spoke` did was drop the stored edge in both directions and push the
  newly solved alignment in its place, so every run on that edge was replaced
  wholesale — a hand correction included. Nothing had noticed because
  **`Source::Hand` was constructed nowhere in the workspace**: the variant was
  defined, serde round-tripped it, and no code path wrote one. The promise had
  never been exercised, which is how a documented guarantee rots without any
  test going red.
  **Half of it was guarded, and the half that was not decided the shape of the
  fix.** `merge_spoke` refused when a stored member's `sha256` differed from
  the one just aligned — "re-solve the work rather than merging into it" — but
  that comparison was against the *`from`* side, the hub the alignment was
  solved against. The spoke was inserted wholesale, and
  `re_aligning_a_spoke_with_no_hand_runs_replaces_it_rather_than_duplicating` pinned that as
  deliberate: re-solving a spoke after fixing its archive is ordinary, so the
  test swaps the spoke's `sha256` and expects the merge to succeed. A merge
  could therefore legitimately replace the very edition a hand correction
  addressed, and carrying hand runs across *that* would resurrect runs
  pointing into slots of an edition that no longer exists — the failure the
  from-side guard exists to prevent, let back in through the other door.
  **The fix divides exactly that way.** Where the spoke's members are
  unchanged, hand runs now carry across and the merge reports how many, as
  `Merged::CarriedHandRuns(n)`. Where the merge would replace a spoke that
  holds hand runs, it is refused instead, naming the spoke and how many
  corrections are at risk. Where the spoke is replaced and holds none, the
  merge proceeds exactly as before.
  **Three things turned up while building it, each worth a sentence.** The
  first `carry_hand_runs` dropped a solved run that merely *overlapped* a hand
  run rather than splitting around it. On a real test it discarded 28 of a
  30-slot run's slots — every slot but the two that were corrected — leaving
  `resolve` to answer `None` for pages no correction had touched. It now
  splits the solved run around each hand run's span instead. The spoke-changed
  test compared whole `Member` structs rather than hashes, so a
  renamed-but-identical archive read as "replaced"; it now compares the
  ordered `sha256` sequence, the idiom the from-side guard's own comment
  already states: "Content, not paths… only the hashes can be compared." And
  the orientation assumption was reachable and silent — `--direct` plus
  `Sidecar::correct`'s own direct-edge branch let a pair be re-merged with the
  names swapped relative to the stored edge, and without a guard the merge
  *succeeded*, reporting `CarriedHandRuns(2)` while misfiling the correction
  against the wrong edition's slot table. Now refused by name.
  **What remains open, deliberately:** what should become of a correction
  addressing bytes that no longer exist. Refusing the merge is what stops that
  being answered silently, not an answer to it. See `notes.md`, "O13 — closes
  here".
- **O14 — `taiyaku-wasm` rebuilt the path between two editions on every
  resolve. Closed.** `Binding::resolve` called `route` per invocation, which
  rebuilt both slot tables and, where the answer composed through the hub,
  recomposed the run lists. `taiyaku_core::Route` exists precisely so a caller
  can pay that once — `route.rs` says so, and puts a number on it: a reader
  paging a 195-page volume would otherwise pay it 195 times. One swap per
  keypress did not notice.
  **The trigger named here was wrong, and M5a found the real one before this
  branch fixed anything.** This item named "a filmstrip resolving per tile" as
  what would notice — but the filmstrip resolves nothing; it consumes a slot
  table built once. What actually rebuilt the route repeatedly is
  `web/src/nearest.ts`'s `nearestPaired`, called while showing the gap state:
  it calls `display` once per candidate slot on either side of a gap, and
  each cross-edition `display` is one `Binding::resolve`. That was up to
  `slotCount` route rebuilds, and because nothing in `web/src` memoised it,
  the same rebuilds repeated on every render of the gap state rather than
  once per swap.
  **`Binding` now memoises the last `(from, to)` pair's `Route`.** A
  `RefCell<Option<CachedRoute>>` on `Binding` — interior mutability because
  `resolve` takes `&self`, safe because a `Binding` is single-threaded by
  construction on the JS side, both stated in `binding.rs`'s own doc comment
  rather than left implied. A different pair replaces it outright rather than
  growing into a map, because the caller this was measured against holds
  `from`/`to` fixed for a whole scan and varies only the slot.
  **`App.tsx` also stopped resolving each gap offer twice.** `nearestPaired`
  called `display` to find the nearest paired slot either side of a gap, and
  `App.tsx` then called `resolveOffered` on that same slot a second time to
  get the page to show — a second `Binding::resolve` for an answer the scan
  already had. `nearestPaired` now returns the resolved landing alongside the
  offered slot (`nearest.ts`'s `NearestOffer`), so displaying an offer costs
  no resolve at all. `resolveOffered` still runs, once, when a reader actually
  takes an offer — `takeOffered` calls it to find where to commit authority —
  which is a different call, made once per click rather than once per render.
  **Measured against the real corpus, not asserted.** Go-Toubun v01's
  `raw-mono` is 195 slots; resolving every one against `en-mono` built the
  route 195 times before the memo and once after — the exact figure
  `route.rs` predicted, reproduced rather than repeated. `raw-mono` slot 1 has
  no counterpart in `en-mono` at all — a genuine gap, not a synthetic one:
  the corpus's own stored run is `"a": "1"` with no `b` side. A scan finding
  its paired neighbours (slots 0 and 2) plus the now-removed second pass
  resolving them again cost four route rebuilds before this fix and one
  after:

  | Case                                                          | Before | After |
  | -------------------------------------------------------------- | -----: | ----: |
  | 195 resolves of one pair (paging all of `raw-mono`)             |    195 |     1 |
  | The real gap at `raw-mono` slot 1: scan + former second pass    |      4 |     1 |

  Two tests in `binding.rs` pin these against a `#[cfg(test)]` counter kept
  only to make a cache hit observable from outside without timing the call —
  `resolving_every_slot_of_one_pair_builds_the_route_once` and
  `a_gap_scan_and_its_former_second_pass_build_the_route_once`. Both failed
  with the pre-memo counts above before `resolve` was changed to route
  through the cache, and pass now.
  **What the memo does not cover: a caller alternating between two pairs.**
  It holds exactly one — `a→b`, `b→a`, `a→b`, ... on successive calls rebuilds
  every time, the same as no cache at all. Nothing in this reader does that
  today: a swap changes what is *shown*, not what `resolve` is asked, and
  `nearestPaired`'s own scan holds `from`/`to` fixed throughout — but a
  caller that interleaved pairs (two gap scans on different edition pairs
  racing, say) would thrash it exactly that way, which is what would reopen
  this.

**The `duplicate-resolution` whole-branch review found four more** — O15 to
O18 — and unlike every group above it, none of them outlived the branch that
found them by long: three were closed on it, and the fourth, whose fix was
larger than the branch, on the commit it asked for:

- **O15 — A flag typed with no usable value was taken as absent, in silence.
  Closed.** `option_value` found the flag and returned whatever argument
  happened to follow it, without asking whether anything did. Both ways that
  can go wrong were quiet. A flag last on the line has nothing after it, so
  `taiyaku inventory releases --min-pages` read as no flag at all and walked
  with the default threshold of 60. A flag followed by another flag took that
  flag as its value, so `taiyaku align a -- b --write --ltr` wrote the sidecar
  to a file named `--ltr` — the direction still applied, since `--ltr` is
  matched separately, so nothing anywhere reported a problem.
  The `duplicate-resolution` branch fixed the instance and left the class:
  `against_members` was changed during the review to *refuse* a valueless
  `--against` rather than fall through to no reference at all, on the argument
  that a flag typed and then ignored is worse than one refused — dropping the
  reference edition changes `decided=` and the rank order, so the silence cost
  a wrong answer rather than a default. None of that argument was about
  `--against`. Thirteen other valued flags, across nineteen call sites in
  `align`, `verify`, `serve`, `inventory` and `duplicates`, still did the
  ignoring, and the pass over all of them is this commit.
  **The two modes did not split evenly, which the deferral had not noticed.**
  Mode A — a flag last on the line — was silent for all thirteen, measured on
  `inventory`: `--min-pages` alone walked with the default and reported two
  candidates as zero, exit 0. Mode B — a flag eating the next flag — was
  already caught by accident wherever the value was parsed for its own sake,
  which is only the two `inventory` flags: `--min-pages --pattern` answered
  "`--pattern` is not a number of pages" and `--pattern --min-pages` answered
  "needs one `{}` where the volume number goes". Everywhere else it either
  surfaced late and elsewhere as a failed read, bind or request, or did not
  surface at all — `--out` would have written the contact sheet to a file named
  after the next flag exactly as `--write` did. So a fix that only validated
  values would have left mode A untouched everywhere and mode B untouched on
  every flag whose value nothing inspects.
  **`--name-a` is the case that decided the rule.** `validate_key` looks like
  it should catch mode B and cannot: `-` is in its safe alphabet, so `--name-b`
  is a well-formed edition key. Measured before the fix — `align … --name-a
  --name-b` wrote a sidecar whose editions were `["--name-b", "ed-b"]`, a key
  that then appears as a path segment in two URLs. So the refusal is **a value
  beginning with `--` is a flag**, applied in `option_value` without asking
  which flags exist, rather than per flag. That is the rule `split_editions`,
  `serve`'s path list and `against_members` already apply to positional
  arguments, so it is this command's existing convention rather than a new one.
  The cost is that a file genuinely named `--something` cannot be named on the
  command line.
  Each flag names what it wanted, the shape `--against` set: `--write needs a
  path, and `--ltr` is another flag`; `--min-pages needs a number of pages, and
  nothing followed it`. **`align` and `verify` now resolve every valued flag
  before either edition is read**, which the key checks already did on the
  argument that a typo should not cost the seconds it takes to fingerprint two
  editions — a refusal arriving after that is most of the cost of the mistake
  anyway. That also removed the second lookup four flags each used to do —
  `--name-a`, `--name-b` and `--title` in `align`, `--sidecar` in `verify`.
  Six tests: four on `option_value` — including the two paths a refusal must
  not break, an absent flag and a present value — and two end to end through
  `align` with member paths that do not exist, so a version that resolved
  options late would fail on a file-not-found instead of ever refusing.
- **O16 — A symlink cycle truncates the inventory walk, and this item was
  opened under the wrong framing. Closed.** It was opened as "a symlink cycle is
  a stack overflow", which is worth saying here rather than correcting quietly,
  because the wrong framing would have aimed the fix at the wrong failure.
  `Path::is_dir()` follows symlinks, so a link back to an ancestor does recurse
  — but Linux caps symlink resolution at 40 hops and returns `ELOOP`, and
  `is_dir()` swallows that into `false`. Measured on this platform rather than
  assumed: past roughly forty levels an unguarded walk does not crash, it
  returns `Ok` with the cyclic subtree reported as childless. That is **silent
  truncation**, the worse of the two failures and the one this project's whole
  idea of a gate that says what it measured exists against. Closed with a depth
  ceiling of 32, deliberately **below** the kernel's 40 so the named error fires
  before the OS masks the condition — a ceiling above it would never have been
  reached. A limit rather than a real cycle check because a cycle check needs a
  `canonicalize` per directory, a syscall apiece on a walk that is 579
  directories on the One Piece collection, to guard against something this rare.
  Two tests hold it: a genuine self-referential symlink under `#[cfg(unix)]`,
  and plain nesting past the ceiling on every platform.
- **O17 — The content-hash confirmation re-read a copy once per candidate it
  was compared against. Closed.** The closure `representatives` uses to confirm
  that two copies with matching signatures really do hold the same page bytes
  hashed both sides from scratch on every call, so a copy compared as the
  earlier half of several pairs paid for its own full read once per comparison:
  O(n²) full reads where O(n), one hash per copy, answers the same question.
  Harmless at the two or three copies a duplicate set usually holds, which is
  every set in this corpus; the bad shape is many copies whose page-size
  multisets collide while their bytes differ, so nothing ever confirms and every
  earlier copy is tried against every later one. Closed by memoising per copy,
  filled **lazily** rather than up front, so a copy whose signature never
  matches anything is still never hashed — the property
  `copies_with_different_signatures_are_never_hashed` already pinned, and the
  one a prefilled table would have broken while every existing assertion stayed
  green. `a_copy_is_hashed_once_however_many_candidates_compare_it` pins the
  other half: five colliding signatures, ten comparisons, one hash apiece.
- **O18 — The rule that a reference edition decides which copy is the impostor
  had no corpus witness, and the witness was already in the corpus. Closed.**
  `--against` moves the decision outside the set: a copy that pairs with the
  reference is this volume however the set treated it, which is the only thing
  that can tell two copies refusing each other apart. That rule was held by unit
  tests while these documents described it as though it had been measured — no
  set in the corpus had both a two-copy refusal and a reference edition to hand,
  so nothing had ever run it over real material. **The witness was there and had
  not been recognised as one.** `incoming/bocchi/` holds three directories named
  as volume 1 of *Bocchi the Rock*, and the third is the *Gaiden* spinoff, a
  different work by a different artist, filed under the main series' name — and
  the same directory holds `en-gaiden-v01.cbz`, the English edition of what that
  file actually is. Resolved twice over the same three copies: with no reference
  the *Gaiden* file is the copy flagged as not this volume, and `hamaji` ranks
  best at 125 slots; against `en-gaiden-v01.cbz` the flagging **inverts** —
  `hamaji` and `krc` pair nothing with the reference and are the flagged ones,
  and the *Gaiden* file is the only copy ranked, at 185 slots. The same three
  files and the opposite answer, decided entirely by what they were judged
  against. That is the argument for `--against` existing, and it is now a gated
  check rather than a claim. See `notes.md`, "What the six gated checks
  measured".

**Two came from running a command against its own usage text** — O19, turned up
on 2026-08-01 while scoping O15 and closed the same day once the question it was
blocked on turned out to be answerable by reading the format, and O20, which came
out of O19's own list of what it had left undone. Both were closed by refusing
rather than by wiring something up, and the pair is why that reads as a rule
here now: a flag that cannot change what a run does is refused, not ignored.

- **O19 — `align` without `--split-spreads` ignored the whole sidecar-writing
  half of its flags, silently. Closed.** `taiyaku align a -- b --write
  out.json` printed the summary, wrote no file, said nothing about not writing
  one, and exited 0. `--write` was read inside `align`'s split-spreads branch;
  the plain branch returns after `report_summary` and never reached it.
  `--title`, `--direct` and `--cache` are unreachable the same way, and
  `--name-a` and `--name-b` are worse than unreachable — they are validated
  *before* the branch and then discarded, so a run that could not have used a
  good key still refuses a bad one.
  **Both this document and the tool's own usage text said otherwise.** The
  synopsis above listed `[--write FILE]` and `[--split-spreads]` as independent
  options, and `--help` listed them on one flat line. Nothing anywhere stated
  the dependency; `corpus/README.md` only says to pass `--split-spreads`
  essentially always, which is why nobody had hit it.
  **This was not O15, and the distinction decided the fix.** In O15 a flag has
  no usable value; here the value is present and usable and the flag was
  ignored anyway, so no amount of validating values catches it.
  **It was opened as a decision — plain path grows a write path, or the
  combination becomes an error — and the format settled it rather than a
  preference.** The plain path has pages where a sidecar wants slots.
  `describe_edition` derives an edition's `spreads` list from its slot table, a
  page being a spread exactly when it appears there twice, and the plain path
  never builds one. So the only sidecar it could write is one claiming
  `"spreads": []`, having never looked — and that claim is not cosmetic:
  `sidecar.md` makes the slot sequence *follow from* `spreads`, so an edition
  that does join spreads would have every run after the first silently
  re-indexed. That is the shape of two failures already recorded here —
  `verify` reporting "192 of 187 pages paired, at 102.7%", and the −8 that was
  "entirely an artifact of joined spreads having no representation". Growing a
  write path would have written an unverified claim into the normative format,
  so the alternative was not cheaper, it was the only sound one.
  Closed by refusing the combination, before either edition is read, with a
  message naming the flag it needs and why: ``--write needs --split-spreads: a
  sidecar records which pages are joined spreads, and only that path detects
  them``. Two tests hold it from both sides — the refusal, and a run with both
  flags getting past argument handling entirely, so the rule cannot be spelled
  "refuse `--write`". `--help` and the synopsis above now state the dependency
  they used to deny.
- **O20 — Six more `align` flags were ignored where they could not take
  effect. Closed.** Opened out of O19's own "deliberately not done" paragraph,
  which said five flags were dead *on the plain path*. Both numbers were wrong,
  and checking rather than repeating them changed the fix. It is six, and four
  of them are dead on **both** paths: `--title`, `--name-a`, `--name-b` and
  `--direct` are read only inside the `--write` block, so `--split-spreads`
  alone skips them exactly as the plain path does. Their rule is therefore
  **needs `--write`**, not needs `--split-spreads` — a run with spreads split
  and no sidecar written ignored a `--title` just as silently.
  The remaining two divide the other way, which a measurement settled rather
  than a reading: `--cache` is **live** on the split path even with nothing
  written — the fingerprint cache directory is created and filled — and inert
  on the plain path, which calls `fingerprint_edition` directly and caches
  nothing. So `--cache` needs `--split-spreads`. `--ltr` is the same shape for
  a different reason: `solve` takes no direction and only slot expansion reads
  one, so on the plain path there is no direction to apply.
  **`--ltr` is the one case where the flag could not have changed the answer**
  rather than merely not having, since a page pairing is direction-free. It is
  refused anyway, because the defect O19 named is "typed and ignored", and a
  reader who passes `--ltr` and is told nothing has been told the tool agreed
  with them. No documented invocation breaks: `corpus/README.md`,
  `docs/sidecar.md` and `scripts/margin-survey.sh` all pass `--split-spreads`.
  One message covers the four sidecar flags, because the reason is identical
  and four wordings would imply four rules; the other two get their own, each
  saying what the plain path does instead, since "needs `--split-spreads`" on
  its own reads as arbitrary. `validate_key` still runs first, so a malformed
  `--name-a` reports its own shape rather than this one — the value is wrong
  whatever the run then does with it. Five tests, four refusals and one guard
  that a full write invocation gets past argument handling untouched, so the
  rule cannot be spelled "refuse these flags".
- **O21 — A misspelled flag was accepted in silence by every command. Closed.**
  O15, O19 and O20 each refuse a *known* flag used wrongly, and all three key
  off the flag's name, so none of them could see a name typed wrong. That left
  one door open, and it led back to the room O19 had just closed: `taiyaku
  align a -- b --writ out.json` reported a plausible alignment, wrote nothing,
  and exited 0 — the exact failure O19 fixed, reached by dropping a letter.
  Worse is the near-miss that still runs: `--split-spread` for
  `--split-spreads` does not fail, it returns a **different** answer, page-level
  where the caller asked for slot-level, and nothing marks it as the wrong one.
  **O15 is what made this checkable at all.** Once a flag's value can never
  itself begin with `--`, every `--`-prefixed token in an argv is unambiguously
  meant to be a flag, so an unrecognised one is a mistake rather than possibly
  somebody's filename. Before that rule the scan would have had to know which
  flags take values and step over them, which is most of an argument parser.
  Each of the eight commands declares its own set rather than sharing a global
  one — `--pattern` is real for `inventory` and a typo for `align`, and one
  list would accept it in both. The message names the whole alphabet rather
  than guessing a nearest match: a typo is usually one character off, but the
  case this exists to catch is a flag the caller believed in, and a single
  guess would be confidently wrong often enough to mislead. `topology` and
  `lookup` take no flags at all and say that instead of trailing off into an
  empty list.
  **One thing fixed alongside it, because it is the same scan:** a flag typed
  before the `--` separator lands in the a-side member list, and used to
  surface as ``--stats: No such file or directory`` — a message naming a file
  the caller never mentioned. It now says the flag is on the wrong side of the
  separator. That one was never silent, only misleading, which is why it was
  not worth an item of its own.
- **O22 — `verify` ignored `--cache` and `--ltr` with no `--sidecar` to apply
  them to. Closed.** O20's rule, in the command O20 did not reach. Both are
  read only from the `--sidecar` arm: without one, `verify` solves the pair
  directly, never calls `cached_edition`, and so consults neither the cache nor
  a reading direction. Found by measurement rather than reading — `verify a --
  b --cache DIR` did not create DIR. It is two flags rather than the one this
  was opened as, which is the second time in this group that checking a count
  rather than repeating it changed the fix.
  `--ltr` is doubly inert here: even *with* a sidecar it is the document's own
  `reading` order that indexes the runs, as the comment at that site already
  said; the flag reaches only `cached_edition`'s spread expansion.

**And then the thing four of them deferred was done: `--split-spreads` is
retired and splitting is the default.** O19 to O22 each ended by recording that
the flag was still opt-in rather than the default `corpus/README.md` describes,
and each declined it as a surface change rather than a defect fix. What changed
the case was those four items themselves. `corpus/README.md` had said to pass it
"essentially always" since before there was a second caller — unhandled spreads
make a correct alignment look broken, which is how several hours went once — and
O19 and O20 then made the default path the one that can do almost nothing: it
cannot write a sidecar, and it refuses six of `align`'s nine flags. **A default
nobody should choose is the same silence this group had been closing, one level
up.**

The flag is **retired rather than accepted as a no-op**, which is the one part
of this that was a choice. Keeping it would have broken the rule the four items
before it established — a flag that cannot change what a run does is refused,
not ignored — and letting `reject_unknown_flags` handle it would have called
yesterday's documented flag a misspelling. So it has its own message, naming
what replaced it. `--no-split-spreads` gives the page-level diagnostic, which
still takes `--stats` and still cannot write a sidecar, for O19's reason
unchanged. The three refusals O19 and O20 introduced now key off
`--no-split-spreads` rather than off the absence of `--split-spreads`; the
reasoning behind each is the same sentence it always was.
`corpus/README.md`, `docs/sidecar.md` and `scripts/margin-survey.sh` no longer
pass it, and the survey was re-run against the corpus to confirm it still
measures — 195 slots on Go-Toubun v01, the split-spread count.

**Everything above about O19 to O22 is left as it was written**, naming
`--split-spreads` in the present tense, because each describes what was true
when it was closed. Only the synopsis at the top of this document and the tool's
own usage text describe the command as it is now.

**One came from measuring what the correction UI produces** — O23, turned up
on 2026-08-04 while measuring M5d's correction UI against the corpus, and not
blocking because nothing yet corrects a work more than once in real reading:

- **O23 — Two hand corrections make a sound alignment report `fragmented`.
  Open.** Measured on the primary corpus volume, correcting `raw-mono`
  against `en-mono`: zero corrections leaves Go-Toubun v01 at 193 paired
  slots, 3 paired runs, average run 64.33, `sound`; one correction takes it to
  192 slots, 6 runs, average run 32.00, still `sound`; two corrections take it
  to 191 slots, 9 runs, average run 21.22, `fragmented`. Each correction adds
  three paired runs and strands one slot, because the 1:1 rule means the
  asserted slot takes its target from whoever held it, and that slot becomes
  a gap: the run it sat in becomes the part before it, the corrected slot
  alone, the part between it and the slot it displaced, and the part after —
  four paired runs where there was one, plus the gap.
  The threshold is 27.0, and it exists to separate needles (1.1–1.7 slots a
  run, measured across twenty-two of them) from confirmed-good pairings
  (36.0–200.0). A twice-corrected volume at 21.2 is neither, so the verdict
  misleads in a way the threshold was never calibrated for.
  **Nothing is changed here.** Altering the fragmentation policy on a guess is
  what this project does not do — the same reasoning that kept O1 open on
  evidence rather than shipping an `ETag` on plausibility.
  **What would decide it:** a work corrected enough times in real reading to
  say whether the verdict misleads a reader in practice, or a measurement
  showing that hand runs and solver runs should be counted differently. See
  `notes.md`, "O23 — opens here".
- **O24 — A solver-written direct edge to a third edition survives that
  edition being replaced. Open.** `merge_spoke` now refuses a merge that would
  replace a spoke holding *hand-corrected* runs on any edge — that is O13's
  fix, extended during this branch's whole-branch review when a direct edge to
  a third edition turned out to escape the original check. What it still does
  not cover is the same edge written by the solver rather than by hand:
  `align --direct` records one, nothing marks it `by: "hand"`, and replacing
  either endpoint leaves it addressing slots of an edition that is no longer
  there. `route` then prefers that stale edge to the hub and reports the
  answer as `Direct`, which is the strongest confidence this format has.
  **Not introduced by the correction UI, and not widened by it.** A direct
  edge has always required a deliberate `align --direct`; what M5d changed is
  that a *hand* one can now be made by clicking a filmstrip tile, and that
  case is the one now guarded. This item is the older half, found by looking
  at the new guard and asking what it does not cover.
  **What would close it:** deciding what a merge owes an edge it did not
  solve. Refusing is the conservative answer and matches O13's stance;
  dropping every direct edge that mentions the replaced edition is the other,
  and is defensible precisely because a solver edge carries no human decision
  to lose. Neither is obviously right, and no corpus work has yet had a
  `--direct` edge and a re-solve in the same document — which is also why this
  has never been felt.

**One came from driving a new surface against the corpus** — O25, turned up on
2026-08-05 during M5c's browser pass, and not blocking because the two panels
it hides are both reachable one keypress earlier:

- **O25 — The settings panel and the whole chrome are unreachable while
  showing a gap. Closed.** `App.tsx`'s `shown.kind === 'gap'` branch returned
  the `Gap` view *instead of* the reader's own tree, so everything mounted
  below it — `Chrome` (filmstrip and readouts), `Settings`, the mode toolbar,
  and until M5c the note panel — was simply not rendered. The keys fired and
  the state flipped; nothing appeared. Found by pressing `a` on a gap and
  getting silence.
  **The early return is gone rather than added to.** `Display` is narrowed once
  (`at`), five values gain a gap form, and the page area draws `Gap` in place of
  a page — so there is now exactly one place in the component a panel can be
  mounted, and a panel added later is reachable in a gap because there is
  nowhere else to put it. Adding two mounts to the gap branch would have closed
  the item as tabled and left the shape that let a third panel be forgotten.
  **The design question this waited on settled smaller than it looked.**
  `goToSlot` is `navigate(state, slot)` against the viewed edition, which is the
  commit `takeOffered` already makes for `Gap`'s offered moves, so a tile click
  in a gap needed no new semantics — only the marking and the scroll position
  were open. The strip shows the viewed edition with **nothing marked**
  (`currentIndex: -1`, correction mode's own convention) and centres on the
  nearest slot that resolves, which is the page `Gap`'s own button offers. The
  two neighbours are deliberately not given the dashed border: dashed means
  "where the alignment says this lands", and in a gap it lands nowhere.
  **Two things were already written and unreachable.** `healthLine`'s
  `no counterpart here` string had no caller that could produce it —
  `healthReadout` passes `hasCounterpart: true` unconditionally, and its own doc
  says why: `Chrome` never mounted in a gap. `gapHealth` reaches it now, and
  reports the authority↔viewed pair rather than hub↔viewed, so the row agrees
  with the verdict sentence `Gap` prints directly above it instead of falling
  silent under it. The readouts otherwise report from the authority — a real
  position in the work, which only the viewed edition has no page for — with the
  edition named, so the change of scale is never silent.
  **Measured in a real browser against the corpus**, on two gaps chosen out of
  the sidecar because the obvious one cannot fail: `raw-mono` slot 1 in
  `en-mono` is a *sound* pairing, so `sound pair · no counterpart here` on
  screen can only have come from `gapHealth`; and `raw-mono` slot 193 in
  `en-fan` anchors at `en-fan` slot 239 — its *last* slot of 240, drawn as the
  last of 235 tiles, since five joined spreads make the two counts differ —
  where `scrollLeft` read 24260 of 25824 rather than 0. See `notes.md`, "O25: the chrome in a gap,
  driven against the corpus", which records all ten checks — including the review
  list's "not reachable from here" marking, run under a backup of the corpus's
  JSON and restored to byte-identical afterwards.
  **The panel that waited on this is unblocked, not shipped.** The credits
  module (§4 below) is registered, its preferences load and save, and mounting
  it is now an ordinary addition to the one mount site rather than a question.

**One came from reviewing a branch that fixed the same hazard next door** —
O26, turned up on 2026-08-07 by the credit-pages whole-branch review, and not
blocking because it is older than that branch and nothing on the branch made
the exposure worse. It closed the same day, on a branch of its own — which is
the point of having deferred it rather than taking it as a drive-by:

- **O26 — `align --write` truncates the sidecar before writing it. Closed.**
  `fs::write` opens for truncation and then writes, so a crash or a full disk
  inside that window leaves a short file where a whole document was. The
  document is the worst one in this project to lose half of: it holds every
  edition of the work, every alignment between them, and every run a reader
  hand-corrected through the reading UI. This is the same hazard O2 closed for
  the reading position, and that `atomic.rs` now owns for every writer inside
  `taiyaku-serve`.
  **`credits --write` had the identical exposure and was fixed on the branch
  that opened this**, which is what made the gap visible. The fix was cheap
  because `taiyaku-cli` already depends on `taiyaku-serve`; the only barrier
  was visibility, `atomic` being a private module and `write` being
  `pub(crate)`. Both are public now and `credits::write_sidecar` goes through
  them, pinned by a test that watches the destination's inode change.
  **`align` now goes through the same call.** `main::align` builds a `Sidecar`
  — merged into whatever the file already held, that part having been fixed
  long ago — and ended on `fs::write(out, sidecar.to_json())`; it ends on
  `write_sidecar`, which is `taiyaku_serve::atomic::write`. **That
  `write_sidecar` is `credits`' own, moved up to the crate root and shared**,
  so `atomic.rs`'s caller tally stays at five: a sixth command-line writer, one
  call site for the two of them. The module doc records that, since a tally of
  *commands* would now be wrong. Both commands' tests still stand, over the two
  documents each puts at risk. The neighbouring
  `fs::write` in `verify`'s contact-sheet path was **not** in scope and is
  unchanged: an HTML report is regenerated from the archives on demand, so half
  of one costs a re-run rather than a document.
  **The estimate said "one line and a test" and was one line short**, in a way
  worth recording because it is a shape that recurs: the line to change sat in
  the middle of `align`, and a test can call a function but cannot reach a
  statement — `align` solves before it writes, so driving that write through
  the command needs real archives with real pages in them. The write moved into
  a named `write_sidecar` first, unchanged, and only then changed. The
  extraction is what made the failure observable.
  **Pinned by `main::tests::writing_an_alignment_replaces_the_sidecar_rather_than_truncating_it`**,
  the shape `credits::tests::writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it`
  established: inode identity, no temp file left beside the sidecar, and the
  document readable whole afterwards. It was watched failing against the
  `fs::write` version first — one inode, unchanged, which is the truncation
  itself — and the fixture is a three-edition star carrying a hand-corrected
  run, because that is what `align --write` rewrites in order to add one spoke
  and therefore what a short file would take with it.
  **The command itself was driven against the corpus**, since a test that calls
  `write_sidecar` does not prove `align` still calls it: two runs on
  `gotoubun-v01` to a scratch path outside the corpus, the second merging a
  second spoke into what the first wrote, where the destination's inode moved
  575378 → 575396 and `lookup` read three editions back off it. See
  `notes.md`, "O26: align --write, driven against the corpus".

**One came from designing another item's fix** — O27, turned up on 2026-08-07
while tracing which render-assigned refs O25's restructure would newly reach,
and not blocking because a write nobody reads changes nothing a reader can see.
It closed the same day, on O25's own branch:

- **O27 — Two refs are held and read by nothing. Closed.**
  `App.tsx` declares `pageUrlRef` beside `pageAreaRef` under one comment
  claiming both are "read by the layout effect below", and assigns
  `pageUrlRef.current` further down. **Neither is read, and there is no layout
  effect in the file at all** — every hook there is a `useEffect`. `pageUrlRef`
  has no reader anywhere in `web/src`; `pageAreaRef` is attached to the page
  area's own `ref=` and its `.current` is never read either. *This bullet
  claimed on 2026-08-07 that `pageAreaRef` was read and that the region-drag
  geometry came off it. It does not: `beginRegion` takes its rectangle off the
  `<img>` via `draftImage`/`pointOn`, and geometry coming off the page area is
  the exact bug M5c fixed — see `notes.md`, "The annotation overlay was placed
  on the page area, not on the page". Corrected by this branch's own
  whole-branch review, before anyone closed the item from a false premise.*
  **It was never a leftover from a removed reader.** `git log -S` finds exactly
  one commit touching either line, `bcafee5` (M5c), and it *adds* both. The ref
  was born dead, most likely because M5c's overlay ended up measuring the image
  it sits over — the fix recorded under "The annotation overlay was placed on
  the page area, not on the page" — rather than working from a URL an effect
  would have had to compare.
  **What it costs, and what it costs to leave.** Nothing at runtime: one string
  assignment per render. The cost is the comment, which tells the next reader
  that a reader exists and sends them looking for it — the same staleness the
  open-items table's symbol-not-line rule exists to avoid, one level down.
  **It was tabled rather than fixed in passing, and then folded in anyway on
  the owner's call.** The argument for a separate branch was the standing one:
  O25 is a restructure of the same function, the remote squashes, and two
  claims in one commit cannot be reverted separately. The owner chose to close
  it on O25's branch instead, which is recorded here rather than deleted,
  because the reasoning is still the reasoning — it was overruled, not wrong.
  **How it closed.** Both refs deleted, along with `pageAreaRef`'s `ref=` on
  the page-area div and the comment that promised a layout effect. They were
  dead in different ways and the difference mattered to the edit: `pageUrlRef`
  was declaration-plus-assignment and nothing else, while `pageAreaRef` was
  attached to a live element, so removing it meant removing the attribute too.
  **The one behaviour that could have depended on the page area's node was
  driven, not reasoned about.** With the refs gone, a real pointer drag across
  the page in a browser still swept out a region — the note's anchor went from
  `whole page` to `raw-mono slot 193 · region`, and the rectangle drew during
  the drag. That is the check worth running, because geometry taken off the
  page area rather than the image is precisely the M5c bug named above. Run
  under a copy of the corpus's JSON, restored to byte-identical afterwards. No
  test covers this; nothing in the suite touches `App.tsx`, which stayed at
  397 passing.

**One came from simulating the reader rather than reading it** — O28, turned up
on 2026-08-07 while checking that the credits panel's skipping could not strand
an arrow, and not blocking because it costs one page of movement in a state a
reader reaches only sideways:

- **O28 — A forward arrow on the last group's second slot moved backward.
  Closed.** `stepToGroup` clamped with `Math.min(Math.max(here + by, 0),
  all.length - 1)` and then landed on the chosen group's *first* slot. At the
  end of the volume those two combined: the clamp kept the reader in the group
  they were already in, and its first slot was behind them.
  **The decision the item was waiting on was taken the way it was framed** —
  the clamp now returns the slot unchanged wherever there is no next group, at
  *both* ends rather than only the one that reversed direction. `stepToGroup`
  holds two rules, and they meet only at a boundary: landing on a group's first
  slot says where an arrow goes when it *moves*, and a press with nowhere to go
  does not move. One rule now covers both ends instead of two rules meeting in
  the middle, which is how this arose.
  **Two things this entry had wrong, both found by running it rather than
  reading it, and both changing what the fix had to be.** It recorded that
  backward at the start "already returns the slot unchanged" — true only from a
  group's *first* slot, which is the only case the existing test covered. From
  the second half of a leading joined spread, backward answered 0: the same
  invisible move, in the direction the reader had pressed, which is why it read
  as correct. And it describes a defect of volumes ending in a joined spread; in
  `two` mode a pair is a group too, so an ordinary six-page volume has it at
  slot 5, which the entry's own first measured line shows and its prose did not
  say.
  **None of the affected presses changed the picture**, which is why a manual
  pass never found it. A group is one image and both slots sit inside it, so the
  arrow appeared to do nothing — correctly, at the end of a volume — while
  silently re-authoring which slot a swap, a note or a written position answers
  from.
  **It predated everything around it**, including group stepping's own arrival
  in `c11c1e9` — that commit's clamp meeting that commit's
  land-on-the-first-slot rule — and reproduced with no `skipped` set at all, so
  the credits work neither caused it nor widened it.
  Three tests pin it, across all three modes and both ends, and were checked by
  restoring the old clamp rather than by having once failed to compile: the two
  boundary tests fail with the predicted values and the third — a second slot
  *away* from a boundary still moving in both directions — passes either way,
  which is what stops the fix being spelled "a second slot never moves". See
  `notes.md`, "O28 — closes here".
  **What is unchanged:** an arrow still cannot *put* a reader on a group's
  second slot. That is the cost `c11c1e9` accepted and this does not revisit;
  a swap, a confirmed correction or a restored progress record are still the
  three ways onto one, and an arrow now leaves a reader there rather than
  moving them off it in whichever direction the nearest boundary faces.

**One came from driving a fix against the corpus after making it** — O29,
turned up on 2026-08-07 while checking that `align --write` still wrote at all
once O26 moved it, and not blocking because it makes a message noisier without
making it wrong:

- **O29 — A failed `--write` named a temp path the caller never typed. Closed.**
  `atomic::write` named the *temporary* when staging failed and the destination
  when the rename failed. Inside the module that is the right distinction — the
  two failures are not the same event. At a command line it was not, because the
  temporary is a filename the caller never chose and cannot look for:

  ```
  $ taiyaku align … --write /nonexistent-dir-xyz/out.json
  taiyaku: /nonexistent-dir-xyz/out.json.tmp.4041762.0: No such file or directory
  ```

  The destination was still legible inside that string and the diagnosis
  unchanged, so nothing was *misleading* — noise, not a wrong answer, which is
  why it was tabled rather than fixed on the branch that found it.
  **It was not new with O26 and not confined to `align`.** `credits --write` had
  answered this way since the branch that fixed it, and the four callers inside
  `taiyaku-serve` since `atomic.rs` existed — where it matters less, because a
  500 does not print a path to anyone. *That last clause is false, and finding
  out why is what decided where this was fixed; see below.* What O26 did was put
  it on a second command-line surface, which is what made it worth writing down.
  Before the change `align` carried `.map_err(|e| format!("{out}: {e}"))` and
  named the destination for both failures.
  **Measured, not reasoned about:** the line above is a real run, recorded in
  `notes.md`, "O26: align --write, driven against the corpus".
  **Closed, and in `atomic::write` rather than at the command line.** The
  decision was framed here as a choice between the two, with the command-line
  wrap preferred as the cheaper: `write_sidecar` is a single place since O26
  merged the two writers. **Reading the callers before building it settled the
  choice the other way, because the paragraph above is wrong about them.** It
  says the four `taiyaku-serve` callers matter less "because a 500 does not
  print a path to anyone". `routes.rs`'s `Failed(message)` arm is
  `(StatusCode::INTERNAL_SERVER_ERROR, message).into_response()` — the string
  *is* the body — so `notes::store`, `progress::store` and the sidecar rewrite
  were all handing the temporary's name to HTTP clients. Only `cover.rs`
  discards it. A command-line wrap would have fixed one surface of four.
  **The fix needed no caller changes at all**, which the framing had also
  missed: `atomic::write` is already given the destination, and merely named
  the temporary instead in one of its two branches. Both now name the
  destination, and the distinction between them — a staging write failing is
  not a rename failing — is carried in words rather than by substituting a
  path the caller cannot look for, since the temporary is deleted before the
  error returns. Driven against the corpus, the scenario at the top of this
  entry now answers ``/nonexistent-dir-xyz/out.json: staging a temporary
  beside it: No such file or directory``. Two tests, one per branch, each
  asserting the absence of `.tmp.` rather than an exact sentence, and both
  checked by sabotage: restoring either branch's old wording fails its own
  test and only its own.

**One came from the same corpus drive, one level up** — O30, turned up on
2026-08-07 when O26's fix was checked against the corpus and the documented way
of doing that turned out to run nothing. It closed the same day, beside O29:

- **O30 — A relative `TAIYAKU_CORPUS` skipped every standing check and
  reported `ok`. Closed.** `cargo test` gives each package's test binary that
  package's own directory as its working directory, not the workspace root, so
  the `TAIYAKU_CORPUS=corpus` that `crates/taiyaku-cli/src/corpus.rs` gave in
  its own header resolved under `crates/taiyaku-cli/` and found nothing. Every
  check then skipped, printed `TAIYAKU_CORPUS unset; skipping` — naming a cause
  that was not the cause, since the variable was set — and libtest reported
  `ok`. **A green run that measured nothing**, which is the one outcome a
  standing check must not have. It is the same failure `atomic.rs`'s test
  comment describes one level down, arriving through the gate rather than
  through a filesystem.
  **`scripts/check-all.sh` already knew.** It canonicalises the path and refuses
  a non-directory, in a comment naming this trap exactly — "the checks then skip
  while this gate says it ran them". So the gate was never wrong; the two module
  headers carrying the trapping command were, and nothing else was checking.
  **The two crates failed differently, and one hid better.** `taiyaku-cli`'s
  root resolver tested `is_dir` and folded the result into the unset branch.
  `taiyaku-serve`'s never looked at the path at all, so each of its checks
  reported `<root>/gotoubun-v01/taiyaku.json absent — skipping` — a real path,
  reading like a corpus missing one volume rather than a root that does not
  exist.
  **How it closed: O15's rule, applied to an environment variable.** A value
  typed with nothing usable behind it is refused rather than read as absent.
  Unset still skips, so CI stays green with no corpus; set-but-not-a-directory
  now panics with the working directory named and the cause spelled out. Both
  headers give a working invocation and point at `check-all.sh` — and **both
  were run as written**, which is the whole lesson of this item: `taiyaku-cli`'s
  measures 2,427 comparisons in 38.71 s, `taiyaku-serve`'s 617 pages and 629
  anchors in 15.13 s. A documented command nobody executes is how this started.
  `taiyaku-serve`'s header had no invocation at all until this item; the bullet
  above claimed it did before that was checked.
  **Pinned by three tests** in `corpus.rs`'s own `tests` module — the first
  ungated tests that file has had, which is the point: they run in CI, where
  every standing check beside them skips. They test the message rather than the
  environment, because setting a variable inside a test is process-global and
  races a parallel runner. What they assert is that the message names the
  variable, the value, and the working-directory cause, and that it never says
  `unset` about a value that is set.
  **Driven, not only tested.** `TAIYAKU_CORPUS=corpus` now fails in both
  crates, naming `/home/davey/projects/taiyaku/crates/taiyaku-cli` and
  `…/crates/taiyaku-serve` respectively; unset still skips green; an absolute
  path still runs all twelve. See `notes.md`, "O26: align --write, driven
  against the corpus", which is where the trap was found.
  **`taiyaku-serve` carries a second copy of the guard deliberately.** Both are
  `#[cfg(test)]` modules in different crates, and exporting test scaffolding
  across a crate boundary to save six lines costs more than it saves. The
  message is pinned where nine of the twelve checks live.

**Series-level identity is manual.** Which two files are the same volume is decided
by the person running the tool.

**Blank pages are still anchored on.** A blank page's dhash is exactly zero, so
every blank in every edition is identical to every other and the solver places
them arbitrarily. The corpus check declines to measure them rather than pretending
they agree; refusing to anchor on low-variance pages is unbuilt. See
`notes.md`, "Blank pages match each other perfectly and lie about it".

**Hub choice is declared, not measured.** `notes.md` argues the best hub is the one
minimising total loss across spokes, and that every input to that decision is
something an alignment already reports. A merge keeps whatever hub the file names.

**Three came from L2a's whole-branch review** — O31 to O33, none blocking because
the detector they describe does not work yet for reasons upstream of all three,
and fixing any of them alone would sharpen an instrument that is measuring the
wrong population:

- **O31 — The axis is decided once per page, where the design says once per
  cluster. Open.** `detect_page` calls `axis_of` over the whole page's
  components, uses that single answer for its one `merge_along`, and stamps it
  on every region. The design's §4 says the opposite, and §1 justifies storing
  `axis` on a region on the grounds that "the clustering already knows it" —
  which is not yet true of the code. **Three consequences, and the third is the
  one that changes how existing evidence should be read.** A page carrying
  vertical dialogue beside a horizontal SFX or narration line cannot report
  both, which is the case the per-cluster rule exists for. The smear runs on
  that one axis, so a page whose global axis resolves wrong is smeared wrong end
  to end — a second plausible contributor to the measured failure that the
  survey's own diagnosis does not name. And **the survey's `v=`/`h=` figures are
  block counts of a per-page decision**, so the effective sample behind "5 of 9
  nominally correct" is pages, not the 148–402 blocks the table shows: the
  evidence is coarser than it reads. Shipped without being recorded as a
  deviation, which the design now corrects in place.
  **All three consequences are addressed, and the item stays open on what
  replaced them.** L2b Task 5 (`8531d05`) smears both axes, unions the results
  and drops what `detect::uncontained` drops. `axis_of` is now `#[cfg(test)]`
  and no longer reached by the detector at all; `detect_page_with` carries the
  axis on the block rather than the page, so a page can report a vertical
  balloon beside a horizontal sound effect, and no page is smeared end to end
  on one resolved direction; and `region_experiment::summarize` counts `v`/`h`
  over surviving blocks, so those figures are block counts of block decisions.
  The paragraph above describes code the branch deleted, and is kept as written
  in the present tense of when it was true.
  **What keeps it open is a residue the branch measured, not a leftover of the
  old design.** Where both smears produce an *identical* box, both joined the
  same strokes identically and there is no axis to discover: the label reported
  is `uncontained`'s tie-break default — the earlier index, which is
  `Axis::Vertical` given the order `detect_page_with` pushes them in — rather
  than a measurement. That is not a corner case. Over the 90 band-free corpus
  pages `region_experiment.rs` samples, 69 blocks survive both before and after
  the change and **13 of them changed axis**, all horizontal to vertical, every
  one on a block whose box, stroke count, median stroke and ink are identical
  in the two runs; the sample's v/h split moved 55/21 to 65/8.
  `uncontained`'s own doc comment states and bounds this. So the design's §1
  justification for storing `axis` on a region — "the clustering already knows
  it" — is still not true of every region, which is the claim this item exists
  to track. **What would close it:** an axis decided by something the
  clustering measures on a duplicate box, or a `Region::axis` able to say it
  does not know.
- **O32 — The `median_area` disqualifier cannot fire. Open.** The L2 design
  names two absolute disqualifiers for a detector, one of them "median region
  area > 0.15 of the page". `MAX_BLOCK_AREA` discards any block above 0.15
  *before* a `Region` is constructed, and `median_area` is computed over the
  regions that survive — so every region satisfies the bound by construction and
  the median is clamped under it whatever the detector does. The survey reported
  it passing on all nine editions while `blocks_per_page` failed on all nine,
  which is how a check that measures nothing reads when everything is broken.
  **What would close it:** taking the block-size check *before* the filter, over
  the merged blocks as `merge_along` returns them. Until then L2b's promotion
  rule has one live disqualifier rather than the two its wording implies, and
  that is the reason this is tracked rather than left as a curiosity — it
  governs a decision nobody has made yet.
  **Narrowed, not closed, by L2b.** The decision it governed has since been
  made: Task 7 promoted the first candidate under a rule stated entirely in 1:1
  matches and spurious-per-page, which never consults `median_area` — so the
  disqualifier being vacuous cost that decision nothing, and `MAX_BLOCK_AREA`'s
  own doc comment now records why the rule is written that way. Two further
  measurements bear on it and neither closes it: exactly **1 of 478** hand-drawn
  truth boxes exceeds 0.15 of the page (a full-page sound effect on
  `kobayashi-v01/raw-mono` p65, at 0.35), so the ceiling excludes 0.2% of the
  target and was never what cost recall; and against the corpus the filter fires
  on 89 of 90 sampled pages, so it is doing real work rather than sitting inert.
  **The code defect this item names is untouched** — every surviving region is
  still clamped under 0.15 before the median is taken, and the fix named above
  has not been written — so it stays open on its own terms rather than being
  ticked because the decision it was blocking went ahead without it.
- **O33 — Two of the five detection constants reject nothing in any test.
  Open.** `MIN_STROKES` and `MIN_INK` are each pinned by a fixture that reaches
  them and is refused by them. `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` are
  not: the panel-border and lone-speck fixtures are both rejected by
  `MIN_STROKES` first, and the blocks that do survive sit at 0.015 against a
  0.15 cap and 180 against a span of 40 — so a factor-of-several error in either
  computation would ship green. Both were hand-checked and are correct today.
  *Half of that sentence is no longer true, corrected here on 2026-08-12 rather
  than rewritten: the **panel border** is not rejected by `MIN_STROKES` any
  more.* Since Task 4 put the band before the smear, a 400×600 border is one
  component spanning the whole page at 10,000‱ and `BAND`'s `hi` of 2,000 drops
  it at `glyph_band`, before `stroke_scale` is reached and before any block
  exists for `MIN_STROKES` to count. `MIN_INK`'s own doc comment states this
  correctly and this entry did not. The item is unaffected — the border is
  still rejected before `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` see it,
  which is what the entry is about — only the named rejecter moves, and the
  lone-speck fixture is still `MIN_STROKES`'s (by 2‱ of band margin, which
  `a_lone_speck_is_not_a_region` now asserts rather than assumes).
  **One further trap worth recording rather than discovering later:**
  `MIN_INK`'s own fixture merges at *exactly* the smear threshold, so a future
  `SMEAR_K` below 2 leaves its strokes unmerged, rejected by `MIN_STROKES`
  instead, and the test still passing while no longer touching the constant it
  is named for.
  **The corpus has since been asked the same question, and it answers a
  different one.** Over 90 sampled pages `MAX_BLOCK_AREA` rejects 166 blocks on
  89 of them and `MIN_BLOCK_STROKE_SPAN` rejects 2 blocks on 1 — so the two
  constants this item names are not inert in practice, `MAX_BLOCK_AREA` least
  of all, and the gap it describes is genuinely in the tests rather than in the
  behaviour. It stays open on those terms. What the same measurement did find
  inert is the constant this item lists as *pinned*: `MIN_INK` rejected nothing,
  on any page. That is O34.

**One came from asking a diagnosis about the case it did not explain** — O34,
found by the measurement that answered why the noisiest edition was the only
one clearing the detector's bar, and not blocking for the same reason O31 to
O33 are not. It has since closed, on the redesign it was waiting for:

- **O34 — `MIN_INK` cannot reject anything on real material. Closed by
  `aa0b479`.** It was
  written as a floor of 0.05 on a block's ink over its own area, and its
  "ink" is the summed **bounding-box** area of the strokes inside — which its
  own doc comment already flags as looser than true coverage. Measured, that
  looseness is total: across the 76 blocks that survived all four filters on
  90 sampled pages, the ratio runs 0.38 to 1.89, **61 of the 76 exceed 1.0**,
  and the filter rejected 0 blocks on 0 pages. Strokes overlap, so summing
  their boxes double-counts paper as ink — and the dense screentone fields
  this filter would be wanted for score 1.03 to 1.29, twenty times the floor,
  rather than anywhere near it. The consequence is not that a wrong block gets
  through today (the filtering is being done by `MAX_BLOCK_AREA`, which fires
  on 89 of 90 pages, and `MIN_STROKES`, which fires on 21), it is that a
  redesign counting on four backstops has two that do anything.
  **What closed it:** the first of the two routes this item named — a true ink
  count. `detect_page_with` keeps the page's own Otsu threshold rather than
  inlining it into `components`, and `ink_in` counts dark pixels inside the
  block against the block's own pixel area, which is the same quantity
  `truth_population.rs::ink_fraction` measured inside 478 hand-drawn boxes at the
  same threshold. The bound is now structural instead of a property of the
  material: a count incremented at most once per pixel of the rectangle it walks
  cannot exceed that rectangle's area, so the `#[expect]` on that filter carries
  a bound that holds where the old one did not. Across the same 90-page sample
  the ratio fell from 0.38–1.89 to **0.09–0.56**, and 61 blocks above 1.0 became
  none.
  **What it rejects now, measured rather than assumed, including the half that
  did not move.** Over the nine labelled editions it rejects 16 spurious blocks,
  taking spurious-per-page from 55.31 to 54.96 — live, but not where the
  precision problem is. **Band-free it still rejects nothing on any of the 90
  sampled pages**, which is the population `region_experiment.rs` measures: at
  0.09–0.56 nothing there comes near 0.05 from above. So the filter is doing work
  in the shipped detector and none in the band-free one, and the item is closed
  on the first of those rather than on both. What it catches is a block that is
  mostly paper — strokes strung far enough apart that the box the smear drew
  around them is nearly empty — and not screentone, which `glyph_band` removes
  upstream; `MIN_INK`'s own doc comment states that distinction, because the
  wording this item was opened against described a different filter.

**O35 to O37 came from the coverage branch, and none of them was introduced by
it.** They are what reading a file closely enough to write tests for it turns
up — which is an argument for the exercise independent of the number it moved.
**All three were fixed on that branch, and the argument against doing so is
worth keeping because it was a good one.** The case for leaving them open: a
coverage branch is a gate and its tests, this repository squash-merges, and a
behaviour change buried in forty-odd commits cannot be reverted on its own.

What decided it the other way was that the branch already carried exactly such
a change by explicit decision — the 412 retry, whose fix is described above and
which is the direct sibling of O35. The revertability cost had been accepted
once already; three more of the same class did not change the shape of the
branch, and leaving a known write-loss filed rather than fixed had a cost of its
own. Recorded so that the next branch faces the question fresh rather than
inheriting this as a precedent: the default for a gate branch is still to file
and not fix.

- **O35 — A `PUT` response with no `ETag` silently killed every later note
  write. Closed by `178f3e5`.** `api.ts::putNotes` returns `etag: res.headers.get('etag') ??
  undefined`, so a response without the header yields `undefined` rather than
  an error. `notesWrite.ts` passes that straight through — `{ kind: 'written',
  doc, etag: fresh.etag }` — `App.tsx` stores it with
  `setNotesEtag(outcome.etag)`, and `writeNotes` opens with `if (!notesDoc ||
  notesEtag === undefined) return`. From then on every note a reader writes
  returns at that line: no request, no notice, no console entry, and the editor
  closes as though it had saved.
  **The same hole was already recognised and fixed one path over.** On the 412
  re-read, `notesWrite.ts` has `if (fresh.etag === undefined) return { kind:
  'refused', notice: message }` — refuse rather than write without a validator.
  That is exactly the right instinct; it was applied to the re-read and not to
  the ordinary response.
  **Not a hypothesis about the server:** it is a statement about what this
  client does if a response omits the header, and the client is the half that
  fails quietly. `taiyaku serve` sets the header today, so nothing misbehaves
  now — which is why this was a latent hole rather than an outage, and why
  there was an argument for filing it at all.
  **What closed it:** the 412 path's own instinct, applied to the ordinary
  response. The single `if (!notesDoc || notesEtag === undefined) return` became
  `writeGate`, which answers with one of three arms rather than with one
  silence — `wait` (no document yet, a load still in flight, and the only arm
  that stays silent because there is nothing yet to tell the reader about),
  `refuse` (a document but no validator: exactly the O35 case, and it now
  carries a notice), and `write`. The correct-behaviour test was watched failing
  against a faithful port of the old guard before the fix went in, and
  `App.test.tsx` observes the refuse notice a reader actually sees (`4105a1c`).
  **It cost a fifth behaviour change, on a branch that recorded four.** On the
  `refused`-after-failed-retry path, `notesWrite.ts` no longer refreshes
  `notesDoc` and `notesEtag` from the 412 re-read; the old code set both before
  recursing. So a write that 412s, and whose retry then also fails, pays one
  extra 412 and one extra re-read before the next write lands. That is a real
  difference in what a reader's machine does, and it was accepted rather than
  overlooked: the cost is one round trip on an already-rare path, and *not*
  adopting the other device's document is what keeps the reader's own notes on
  screen instead of silently replacing them with a copy they never saw. It is
  recorded here because the branch's other four behaviour changes are recorded,
  and this one should not be the exception for having been a consequence rather
  than an intention.

- **O36 — The write debounces outlived the component. Closed by `4303dee`.** `writeProgress`
  and `saveNotes` are `debounce` handles with timers up to 700 ms, and no
  unmount path cancels either. An unmounted `App` can therefore still issue a
  `putProgress` or `putNotes` against a work the reader has left.
  **A reviewer established this is a two-line fix rather than an
  impossibility**, and that finding is the reason it is written down instead of
  waved off: `hooks.ts::useFlushOnHide` already runs a cleanup on unmount, and
  that cleanup today does nothing but remove its two listeners; `debounce`
  already exposes both `flush()` and `cancel()`, and no unmount path calls
  either. The pieces are all present and unconnected.
  **Nothing observed it, which is the whole reason filing it was arguable**:
  `App` is the SPA root and never unmounts, so no reader could reach the
  behaviour. It becomes real the moment a route, a work switch, or a test
  harness mounts and unmounts it — and a test harness is now something this
  repository has, which is what turned the argument.
  **What closed it:** the decision the diagnosis said was the work — and it came
  out **flush at both**, made per call site rather than once for the pair.
  `useFlushOnHide`'s cleanup now flushes `writeProgress` and `saveNotes` on
  unmount, because cancelling `writeProgress` drops the reader's last page turn
  and cancelling `saveNotes` destroys up to 700 ms of prose that exists nowhere
  else: a debounce window that has not elapsed is not evidence that the reader
  changed their mind. `cancel()` keeps its place where the pending write is
  genuinely *stale* rather than merely pending — at `stepTo`, where a newer
  position is about to be written over it anyway, and at the panel's delete,
  where the note the pending write describes is gone. The decision was indeed
  the work; the code was the two lines a reviewer had promised.

- **O37 — `reviewNotes`'s doc comment described an arm nothing can reach.
  Closed by `f4047b7`.** It said `a` "closes the editor if that is what is open, so `a` always
  means 'show me the list'". `keyplan.ts` disagrees and is right: `if
  (guards.panel === 'edit') return undefined` refuses every intent but
  `Escape` while the editor is open, so `a` never reaches `reviewNotes` at all
  and `toggleReview`'s close arm is unreachable from the keyboard.
  **The behaviour the code has is the one a reader wants** — `a` is a letter,
  and someone typing a note should get the letter rather than lose the panel —
  so this is a documentation defect, not a behaviour one. The file already
  states the correct rule a dozen lines above, in `startNote`'s comment: "no
  module key reaches this at all while the editor is open (`keyPlan`'s `panel
  === 'edit'` refusal)". Two comments in one file contradict each other, and the
  wrong one sits on the function a reader would check.
  **What closed it:** the comment was rewritten to describe the toggle the
  function actually performs, which was the cheapest fix of the three — and
  chasing the *claim* rather than the file turned up a second copy of it.
  `notes.ts`'s own `toggleReview` doc repeated `App.tsx`'s wrong sentence
  unchanged, and was corrected with it in `7cab6ec`. The one judgement in the
  item went the conservative way: `toggleReview`'s close arm stays. It is
  unreachable from the keyboard, not unreachable from the function, and
  deleting a correct branch to make a comment true is the wrong direction of
  fix. This was the only one of the three where fixing it found more of itself
  somewhere else, which is the argument for grepping the sentence rather than
  editing the line.

**O38 and O39 came from the mutation branch, and neither is a mutant.** Both
were found *beside* the survivors rather than among them: one by a reviewer
declining to take a claim on trust and writing a probe instead, the other by a
reviewer asking what a survivor's disposition would mean if it ever happened in
CI. That is worth recording because it is not what mutation testing is
advertised to do. The tool finds unverified lines; what actually surfaced these
two was a person reading the code around a line the tool had pointed at.

- **O38 — `unpaired_runs` treats a gap run's own pages as covered. Closed.** The
  function answers "which pages of `edition` does the alignment against
  `against` never reach", and it builds that answer by collecting every page
  any run mentions and complementing it. The collection is unconditional:

      let range = match mine_is_a {
          true => Some(run.a),
          false => run.b,
      };

  The `false` arm is right by construction — a gap run has no `b`, so `run.b`
  is `None` and the gap's pages stay uncollected. The `true` arm is wrong for
  exactly the same case: a gap run's `a` span is precisely the pages that have
  *no* counterpart, and marking them "mentioned" is the opposite of what the
  function is for. Demonstrated rather than argued, on a two-member fixture
  whose middle run is `{"a":"3"}` with no `b`: the `to` side reports
  `[UnpairedRun { pages: 3..4, … }]` and the `from` side reports `[]`.
  **Reachable today.** `main.rs:1268` stores edges as `from: a_name`, defaulting
  to `sidecar.hub`, so stored edges run hub→spoke; `credits.rs:246` lets
  `--against` bypass the hub refusal, and `credits.rs:299` then calls
  `unpaired_runs(doc, edition, counterpart)`. So `taiyaku credits --edition
  <hub> --against <spoke>` takes the defective arm and prints zero candidates
  for a volume that has them.
  *The two `--edition <hub>` invocations above are wrong and are kept as
  written: `credits` has no such flag. Its usage is `credits <member>...
  --sidecar FILE [--against EDITION]`, and the edition is identified by
  fingerprinting the archives passed, not named on the command line. The
  mechanism the passage describes — hub as the `from` side, `--against`
  bypassing the hub refusal, `unpaired_runs(doc, edition, counterpart)` — is
  exactly right, and the drive below ran the corrected form.*
  **Why it was open rather than fixed:** the branch that found it was a test-only
  branch, and this is a behaviour change. The fix is one expression —
  `run.b.map(|_| run.a)` — but it wanted its own commit and its own corpus drive,
  because "prints nothing" is the failure mode that hides how long it has been
  wrong. It fails silently in the found-nothing direction, corrupts no stored
  document, and no current caller in the corpus checks hits it, which is why it
  was judged not worth blocking the branch on.
  **Closed, and the corpus drive is the reason it was worth doing separately.**
  The fix is the one expression the entry predicted. The drive was not: twelve
  volumes across four series, each aligned raw-mono → en-mono and asked from the
  hub side, **every one of which answered `runs=0` before and the stored gap
  count after** — 26 gap runs spanning 36 raw pages, none of them reportable by
  the side a reader would naturally ask. The estimate that "no current caller in
  the corpus checks hits it" was true only because no corpus check asks the hub;
  every volume in the corpus has the material. And it was hiding a real result
  rather than only runs: `kobayashi-v01` raw page 145 of 146 comes back
  `at-end yes … credits`, where the same command previously printed a bare
  header. The `before` column was built from a clean worktree at `ddb6149`
  rather than by reverting the fix in place, so no number depends on an edit
  having landed. **The corpus now watches this arm rather than only having been
asked once**: `gotoubun_hub_reports_the_pages_its_english_edition_never_reaches`
is a thirteenth standing check, and it was confirmed failing against `ddb6149`
with the new test applied to it — `left: []`, `right: [1, 6]`. Both existing
credits checks ask the `to` side, so neither could ever have seen this.
`notes.md`, "O38: the arm that answered nothing, on every
  volume in the corpus", is the record — including why
  `each_end_of_a_directed_edge_reports_its_own_gaps`, which executes the
  defective arm and was hardened in `ddb6149` for being too weak, still cannot
  fail on this: its only run is paired, so both arms agree.

- **O39 — nothing bounded a test's runtime, so a hang was not a failure. Closed.**
  Mutation testing produced this one by accident. `cargo-mutants` reports a
  mutant that stops a loop terminating as `TIMEOUT` rather than `MISSED`, in its
  own bucket, and exits 3 — it is *not* counted as caught, though it is real
  discrimination in the sense that the mutant cannot produce a silently wrong
  alignment. Asking whether that disposition is good enough turned up the
  gap: **there is no `.config/nextest.toml` anywhere in the repository, no
  `timeout-minutes` in `.forgejo/workflows/ci.yml`, and CI does not run nextest
  at all** — the `test` job runs `cargo llvm-cov`, which wraps `cargo test`. So
  the auto-timeout that makes a hang legible locally exists only inside
  `cargo-mutants`, and a regression of that shape reaching `main` would hang the
  forge until the runner's own limit rather than fail it.
  This branch takes `solve.rs` from one tracked timeout to two: a test added for
  the traceback converted the `-=` → `/=` mutant in `solve`'s `SkipB` arm from
  `MISSED` to `TIMEOUT`, which is an improvement in verification and a standing
  cost of about 21 s on every future run of that file — measured at 20.5 s for
  that mutant and 21.5 s for the one already there, against the tool's
  auto-selected 20 s bound.
  **Why it was open rather than fixed:** it was a CI change, and the branch that
  found it changed no CI. It also wanted a measured number rather than a guessed
  one — the whole workspace suite was 552 tests in 0.76 s at that branch's head,
  so a per-test bound was cheap to set, but the right value had to come from the
  slowest real test rather than from a round figure.
  **Closed, by two bounds sized from a measurement rather than a guess.**
  Bisecting the whole workspace under a temporary short `slow-timeout` found
  nothing slower than 0.75 s in 639 tests, so `.config/nextest.toml` carries a
  30 s warn and a 60 s `terminate-after` — 40x and 80x the slowest test that
  exists — and CI now runs the suite under `cargo llvm-cov nextest` rather than
  plain `cargo llvm-cov`. Every job also carries `timeout-minutes: 10`, against
  a worst observed job duration of 134 s. **The per-test half was triggered on
  the forge itself, not only reasoned about**: PR #47 carried a deliberate hang
  for one run, and the `test` job failed at 153 s on that commit and passed at
  117 s on its exact revert — both far under the ten-minute job bound, so only
  the per-test kill fired. **The `timeout-minutes` half is untriggered rather
  than untested, and deliberately so**: proving it would fire means hanging a
  job for the full ten minutes on the shared runner, where the per-test bound
  above could be — and was — triggered in about a minute. **What was and was
  not directly observed is worth separating honestly, and that has changed
  since this was first written.** Against Forgejo `15.0.5~gitea-1.22.0`, this
  build registered no job-log endpoint, so the CI log text itself could not be
  retrieved; the log excerpt on record came from running the same `ci.yml`
  command locally against the hanging test, not from reading the forge's own
  output, and what the forge itself showed directly was only the
  `test`-job-only failure at 153 s on the hang commit and the success at 117 s
  on its revert. The instance has since been upgraded to
  `16.0.2+gitea-1.22.0`, which registers `/actions/runs/{id}/jobs` and
  `/actions/jobs/{id}/logs`; the log for job 3121 (run 213, the hang commit)
  has now been retrieved directly from the forge, and it shows
  `SLOW [> 30.000s]`, `TERMINATING [> 60.000s]`, `TIMEOUT [ 60.003s]` and
  `Summary [ 61.243s] 640 tests run: 639 passed, 1 timed out` — the values
  `.config/nextest.toml` configures, read on the runner rather than stood in
  for. What was inferred is now observed. `notes.md`, "O39: what the slowest
  test actually is, and the two bounds sized
  from it", is the record, including why nextest's process-per-test isolation
  makes coverage measurement nondeterministic by a few lines without moving
  either coverage gate.

## What comes next

Roughly in order. All four are now settled — one closed by a measurement that
came out against shipping it, one shipped as two commands, the third the large
piece, shipped across four milestones, and the fourth shipped as an eighth
command plus a field in the format — and they stay here rather than being
deleted, for the reason the open-items table gives: a list that only ever grows
says nothing about whether anything lands. **All four are now shipped whole**:
the fourth's reader half — the panel and a `skip` that governs something —
landed on 2026-08-07 with O25, which had blocked it, closed the same day.

**A fifth arrived from below rather than from this list.** L1 geometry sat under
"Later" from 2026-07-30, the day this file was written, until it shipped on
2026-08-08; it is §5. The entry that advertised it is kept and corrected at the
foot of "Later" rather than deleted, because both halves of what it promised
turned out to be wrong and that is worth reading. Everything still genuinely
unbuilt is under "Later".

**A sixth was never on any list, here or under "Later".** It is three reader
complaints raised after L1 shipped — pages not filling the page, a gap down the
middle of a spread, and no way to find out what the keys do — grouped onto one
branch because each is small and none touches the alignment. It is §6. That it
came from reading the corpus rather than from a review is the pattern `notes.md`
keeps recording: the browser is where the reader's own defects surface, and no
amount of unit testing substitutes.

### 1. Refuse on margins, not just counts — measured, and margin does not fit

The candidate fix was measured against the corpus rather than shipped. Margins
are quantised to whole 256ths, and the corpus's one confirmed needle (Color
Walk, margin 0.004) sits exactly one bit below its weakest confirmed-good
pairing (Monster's 2:1 repagination, margin 0.008). No safety factor fits in a
one-bit gap, so no margin threshold ships. Average run length carries the case
instead — every confirmed-good pairing at 36.0 to 200.0 slots, everything else
at 6.8 or below — and on this corpus runs per hundred pages separates as well,
once refusals are gated on coverage; the swap was made because run length is
stated in slots rather than per hundred pages, not because the old measure
could not tell these rows apart. `verdict()` already called the needle case
`Fragmented`; both `align` paths now print that verdict, so the case that
opened this item is no longer silent.
**Closed.** Twenty-two needles have since been measured, and they settle it
harder than the survey could: a needle outscores four confirmed-good pairings
on margin and seven on separation, so neither calibration statistic can be made
to refuse this case. Only run structure separates, by a factor of twenty. What
remains open is narrower and belongs to the reader rather than the solver —
`Fragmented` is reported and nothing refuses on it, so a reader consuming a
sidecar has to look at the verdict rather than assume acceptance means
correspondence.

### 2. Duplicate resolution as a command — shipped, and one word in the protocol was wrong

The protocol was worked out by hand across 109 volumes and is now two commands.
`inventory` finds the sets in a release directory; `duplicates` resolves one set
into a decision. On the One Piece collection the hand run took 19 duplicate sets
down to 13 real choices, with 2 mislabelled files flagged rather than silently
ranked, and the commands run the same three steps:

1. **Signature** — file count, total bytes, sorted size list, confirmed by a
   content hash over page bytes wherever it matches. Settles exact duplicates
   without decoding a page, and a copy it settles is never fingerprinted at all.
2. **All-pairs alignment within the set** — not everything against the first
   copy, which hides clusters. Spreads always split, with no flag to turn that
   off, because the headline case in this collection is 200 images against 101.
3. **Cluster on identity, flag what is not this volume, and rank the rest** by
   content first — slots, then pairings against an `--against` reference — and
   only then by how cleanly it aligns. What is not this volume is whatever
   refused against every other copy; with a reference edition it is whatever
   pairs nothing with the reference, which is the only thing that can answer a
   two-copy set where each refuses the other.

**Say what shipped, not what the design sketched.** `notes.md` used *identity*
two ways, and the command uses only the stricter one. `align` reports unmatched
slots for the **A side alone**, so a pair that covers all of A prints as
"nothing unmatched" whether or not B carries slots of its own that nothing
reaches; read that way, containment looks like identity. The command asks both
sides and reports five relations — identity, `within`, `contains`, distinct,
refused. Of the five pairs in the corpus that come within two slots of covering
each other, **three are containment and two are identity**: One Piece v092 is
200 slots wholly within 202, Bocchi's two real scans are 124 within 125, and
Chainsaw Man's `src-a` is 198 within `src-b`'s 199, while Chainsaw Man's two
198-slot scans and One Piece v077's c1 and c2 do cover each other on both sides.
`notes.md` called four of the five identity. Clustering merges on identity
alone, because a container holds content the other copy lacks and collapsing the
two would discard the very difference the choice is about — the ranking already
prefers the container, on slot count.

**Six gated corpus checks measure this, and all six pass.** v077 resolves to
clusters `{0}` and `{1, 2}`, the identity pair at 1 run and 234 of 234 against
104 and 100 fragmented runs for either copy against c0. v092 comes out
containment, two clusters, c1 first on 202 slots. Bocchi's *Gaiden* file refuses
against both real scans, is flagged, and is excluded from the ranking. A sixth
runs those same three Bocchi copies a second time, against the *Gaiden* file's
own English edition, and measures the flagging inverting — the two real scans
flagged and the *Gaiden* file the only one ranked, at 185 slots against the 125
it ranked behind with no reference. Four Chainsaw Man raws against the English
release rank `src-b` first on 199 slots, with `src-a` and `src-c` collapsed into
one identity cluster. `inventory` over `op-duplicates` finds 34 candidates
across 16 volumes, every one of them duplicated. The full figures are in
`notes.md`, "Duplicate resolution as a command, and half the identity claims
above are containment".

**What it does not do, stated with the example rather than as an abstraction.**
The ranking prefers whichever copy holds more material, including material that
may not belong: on v077 the command names copy 0 the best, on 236 slots against
234 — and copy 0 is the different-crop outlier that fragments against both of
the others. The pairs table above the ranking shows that plainly and `--against`
a reference edition settles it, but a ranking read on its own picks the outlier
here. `inventory` has the matching caveat: it groups on the deepest directory
that holds a whole edition, which is not always the copy directory a person
would name, and `--pattern` matches against that directory's name rather than
the copy's.

### 3. The reader — shipped

A PWA consuming the sidecar, with the correction UI as part of it rather than
after it. This was the multi-week piece and the point of everything above, and
every stage it was split into — M5a, M5b, M5c, M5d — has now landed.

**Its blocker is cleared.** Until 2026-07-30 there was no way to produce a
sidecar with three editions and no way to resolve through one, so on any work
with more than two editions — which the primary development corpus is, at five —
the reader could not have answered "show me this page in edition C". `align
--write` now merges and `resolve` now composes.

**The swap UX is settled, not open — this line previously said `notes.md` left
it undecided, which had stopped being true.** `Space` toggles between the
current edition and the last one and `1`..`9` jump directly, settled on
2026-07-30 and reopened and re-settled on 2026-08-01: the case against a
hold-to-open picker was "it costs the one-key promise," which only holds on a
device with a key, so a later brainstorm's picker did not lose the argument, it
answered a different device. Desktop-first was confirmed the same day, which is
what keeps the toggle for this reader; the picker moves rather than dying,
reassigned to a future native iOS reader where a sheet is the right gesture for
a device with no key to promise. See
`docs/superpowers/specs/2026-07-30-reader-design.md`, "Swap is a toggle key
plus number keys."

**M1, M2 and M3 have landed.** M1 moved page reading into `taiyaku-read`, so the
aligner and anything serving pages derive the normative sequence from one place,
and added addressing a single page by index — serving page 47 of a 195-page
volume costs one page rather than the volume. M2 added `taiyaku-serve` and the
`serve` command: the work list, the sidecar verbatim, a page by index with its
content type, and a reading position stored beside the sidecar, all behind a
`PageSource` trait so running beside Stump and replacing it are the same code
above that line. A gated corpus check compares every page the server serves
against the page the aligner sequenced — 617 of them on Go-Toubun v01, no
disagreement.

M3 added Stump support and an OPDS 1.2 + PSE feed. **Say what shipped, not what
the design sketched:** the design named a third `PageSource` implementation,
`StumpLibrary`, alongside `LocalLibrary` and a composite. It was never built,
because it could not have answered anything on its own — resolving a member to
a Stump book needs the resolution map, which only the composite holds, so a
standalone Stump-only source would have been unusable by construction. What
shipped is `CompositeSource`, which resolves each member to whichever source
holds it, with the Stump-backed branch (GraphQL to resolve, REST to fetch
bytes) living inline inside it. `PageSource` has **two** implementations, not
three.

**What would make the third one worth building**, recorded so the decision can
be revisited on evidence rather than re-argued: a work whose members live only
in Stump and never locally. That is not reachable today for a reason deeper
than the trait — `resolve_member` needs the member's *local* file, for its size
and for the page-hash confirmation that makes a match trustworthy, so a member
taiyaku does not hold can never resolve at all. M3 therefore offloads page
reads for files you already have; it does not let taiyaku serve a work it does
not have. Serving one would need a different way to match a member to a book,
which is its own design question and wants its own pass — at which point a
standalone Stump source stops being nomenclature and starts being load-bearing. **Say what the feed actually earns, not what the milestone originally
claimed:** M3's stated end state was "Panels reads the library", but that was
already true without taiyaku — this Stump instance serves OPDS-PSE itself, and
a third-party reader could point at it today. What taiyaku's feed adds is
grouping several editions under one work, which neither Stump nor Komga can
express: on this instance `Go-Toubun no Hanayome` and its Full Color Edition
are two unrelated 14-book series, with no link between them and no way to make
one inside either server. The gated `PageSource` equivalence check measured
this for real against the live instance: 187 pages of `en-mono` agree
byte-for-byte between `LocalLibrary` and the composite's Stump-backed path,
and 8 members — the raw and all seven fan chapters — are absent from Stump and
correctly recorded absent rather than retried per page. See `notes.md` for the
coverage table and what the instance exposes.

**M4 has landed, and it is the smallest of them.** `Route::resolve_slot` makes
the slot rather than the page the thing a reader resolves, which matters exactly
where a run ends mid-spread: in the primary sidecar, `en-mono`'s page 4 is a
joined spread whose right half pairs with raw 5 and whose left half pairs with
nothing, so a reader resolving the page instead of the half would show one and
call it the other. `taiyaku-wasm` puts that in the browser's reach without
`taiyaku-core` taking a `wasm-bindgen` dependency, and the gate now builds both
for `wasm32-unknown-unknown` — the binding for a second reason, since a
signature that cannot cross the JS boundary compiles natively and fails only
there. What was **not** claimed at the time: nothing had executed the module
under a wasm runtime. The agreement check compares the binding against the
core on every slot of every ordered pair of a two-edition fixture, which
catches conversion drift and not codegen.
**M5a closed that gap by running it.** `scripts/smoke-wasm.mjs` loads the
compiled `.wasm`, parses the fixture and checks six properties against it —
`editions`, both slot counts, a half-aware `slotAt`, `indexOf` and one
`resolveSlot` — against the same values `binding.rs` already pins natively.
All six passed, and the generated `.d.ts` matched what the plan had assumed
about the binding's shape. See `notes.md`, "M5a, measured directly: the
reader's own evidence, not the scratch ledger's". **What is still not
claimed:** CI does not run this script — `docs/ci.md` records why — so a
codegen break would still go green there; the discipline stays a local one,
run after any change to `lib.rs`.

**The Panels measurement has started, and found a defect before it found a
number.** Panels read from this server over the LAN on 2026-07-31 — see
`notes.md`, "Panels, measured: the feed was one-based and should not have been".

The first session's finding was not a performance figure but a correctness bug:
**OPDS-PSE counts pages from 0 and taiyaku's feed counted from 1**, so Panels
met a 404 on opening every book, and every page it then displayed was the one
before the page it believed it was on. Thirty per cent of the session's requests
were that 404. Fixed, and the test that used to assert the wrong convention now
asserts the right one at both ends of the range.

**The targets are now measured**, over a deliberate fast-flick session of 177
page requests. The server is not the constraint by a wide margin: median
processing 1.43 ms against a tightest client request rate of 40 ms. Panels
issues no concurrent page requests — zero overlaps, and the arrival floor holds
at 40 ms even flicking as fast as the UI allows — and never asks for a reduced
size, so it takes whatever resolution the server holds, 247 KB a page here.

One thing is deliberately **not** claimed: whether Panels *blocks* on each
response. At 1.43 ms no request was ever still open when the next arrived, so a
blocking client and a UI-paced one are indistinguishable in this log. Measuring
it would need a deliberately slowed response, which is not worth building.

**M5a has landed.** The swap works: `Space` toggles between the current
edition and the last one, `1`..`9` jump directly, and a swap never writes
position — round-tripping through any number of editions is lossless by
construction, not by care. All three reading modes ship — single page,
two-page spread (right-page-first under `rtl`), continuous scroll — with a
joined spread drawn once, not as two halves, in every one of them. Landing on
a slot with no counterpart in the edition just swapped to reports a gap rather
than silently moving to the nearest slot, and offers the nearest paired slot
either side as a move the reader takes deliberately rather than one that
happens to it. A floating chrome shell — a filmstrip and a slot readout,
brought in and taken away together by `Tab` — never reflows the page
underneath it, and its tiles are served by a per-page thumbnail route measured
against the real corpus (O11, below). Reading position is restored on load
from the server and written back on `navigate` — debounced, so arrowing
through a run of pages is one `PUT`, not one per keypress — and never on
`swap`, which has nothing to record.

**What M5a leaves open.** There is no prefetch and no double buffer: a fast
flick between adjacent pages can still show a blank or half-decoded frame
before the next one paints, closed in M5b behind two `<img>` elements and
`img.decode()`. Continuous-scroll mode does not write reading position back
either: entering the mode scrolls to where you were, but hand-scrolling never
updates the stored slot, so a reader who reads by scrolling and closes the tab
loses their place — an `IntersectionObserver` is the fix, and it belongs to a
later milestone, same as prefetch.

**What M5b's decode gate leaves open, in two-page mode.** The gate is per
`PageView` instance, not per pair: an ordinary spread's two halves each run
`presentWhenDecoded` independently, so a turn can present one freshly-decoded
half beside one still-showing old half for as long as the slower of the two
takes to decode. That is not the defect the gate exists to close — neither
half is ever blank or half-painted on its own, which is the guarantee
`decode.ts` actually makes — but simultaneity across a pair is not something
this build guarantees, and no comment here should be read as claiming it is.
Left independent deliberately rather than gated together: gating would make
the faster half wait on the slower one on every ordinary turn, a real latency
cost paid on every pair for a mismatch nothing has measured a reader
noticing, particularly now that `App.tsx`'s separate swap and prefetch decode
queues (`swapQueue`/`prefetchQueue`) keep either half's decode from being
starved by prefetch traffic in the first place. A measurement showing the
mismatch is large enough to matter is the reason to revisit this, not a
guess made ahead of one — see `App.tsx`'s `renderGroup` for the same
reasoning kept beside the code it describes.

**The filmstrip's tiles do not reopen O9 — this paragraph previously said
they could, and that was wrong.** Each tile resolves its own thumbnail, which
on a cold strip is a burst of up to N requests for N *distinct* pages —
structurally the same shape as the cover burst O9 already measured and
declined to guard: one request per member, nothing to coalesce. O9's own
reopening condition is concurrent requests **for one key**, which this is not.
Nothing on this branch reopens O9; it stays open on the reasoning already
recorded there. The mitigation this paragraph also claimed for the burst
turned out weaker than stated: measured directly, the tiles' `loading="lazy"`
does not defer thumbnail requests at all in the environment tested, so it
bounds nothing about the size of a cold-strip burst. See `notes.md`, "M5a,
measured directly: the reader's own evidence, not the scratch ledger's".

**O14, not O9, is the item M5a actually gave a trigger to, and from a caller
nobody had named.** `web/src/nearest.ts`'s `nearestPaired` calls `display`
once per candidate slot either side of a gap, and each cross-edition `display`
is one `Binding::resolve` — up to `slotCount` route rebuilds, unmemoised, on
every render of the gap state. See O14's entry above.

**M5b has landed.** Every reading behaviour M5a shipped as fixed code is now
a `ReaderModule` against one declarative contract (`module.ts`): a `defaults`
object, an optional set of claimed keys, checked for a keybinding collision
or a duplicate id at import time rather than at runtime. Five modules exist
against it — `direction` (a per-edition reading-direction override, `d`,
layered over `work.reading` and never written back to it), `motion`
(animation on/off and transition duration, an empty style object rather than
an inline `transition: none` so a reader's own `prefers-reduced-motion` still
governs when animation is off), `regroup` (parity breaks a reader inserts by
hand, `b`, for the one case a joined spread cannot derive), `readouts` (the
four lines above, each independently switchable, with an unknown alignment
verdict reported as `verdict unknown` rather than silence that could be
mistaken for `sound`), and `prefetch` (a tunable window ahead and behind the
current slot, decoded off-screen through `decode.ts`'s double-buffered
`<img>` pair and swapped in only once `img.decode()` resolves — the fast
flick M5a left flickering). `Sidecar::chapter_at` in `taiyaku-core`, carried
across the `wasm-bindgen` boundary and into `Resolver`, feeds the chapter
readout; it names a member, not a chapter number, and the one chapter-
organised edition in the primary corpus (`en-fan`, reached from the hub by a
`fragmented` alignment) is marked `~` in the readout rather than shown as
settled fact, for the reason the twenty-two needles already established.
`routes.rs` now sends an `ETag` on every page response, keyed on a member's
`sha256` and its offset within that member, which is what let O1 be measured
rather than argued (below). `Binding` memoises its last-resolved route,
closing O14 (above).

**Task 11 closes the milestone: the settings panel, generated rather than
hand-written per module, and every module verified switching off on real
material.** `Settings.tsx` renders one section per registered module with a
control inferred from each `defaults` key's runtime type — boolean → check-
box, number → numeric input, string → text (a select is what the design
calls for once a module declares one with options; none does yet, and the
contract carries no field to declare options against, so that path is not
built ahead of a case that exists only in the design) — never branching on
which module or key it is looking at. A sixth registry entry, `settings`,
claims one key (`s`) for the panel itself the same way any module claims a
key, its own `defaults` empty, so the generated panel renders no section for
it. Two of the five modules hold a preference keyed per edition
(`direction`'s `override`, `regroup`'s `breaks`) rather than a flat value;
the panel has no way to know "edition" is what a given `Record` is keyed by
without being told per module, so these render a read-only entry count and
are cleared by the section's own Reset rather than edited field by field —
the module's own key (`d`, `b`) still edits one entry at a time, as before
this task. All five modules were then driven in a real browser against
`corpus/gotoubun-v01`: each one's behaviour stopped when switched off, took
effect when tuned, and survived a reload, with no remnant left on screen
when off. `prefetch`'s `maxConcurrent` needed the decode queues themselves
to change from a value fixed at mount to one rebuilt when the setting
changes — without that, tuning it in the panel would have been a control
that did nothing until the next reload, the exact kind of generated-but-
broken input this milestone was built to avoid. See `notes.md`, "M5b Task
11: the settings panel, generated, and all five modules measured switching
off", for the module-by-module table.

**A whole-branch review before this branch merged found "generated rather
than hand-written per module" true only of `Settings.tsx`, and fixed the gap
rather than merely narrowing the sentence.** `App.tsx`'s own preference
state — a `useState`/`loadPrefs` pair, a `savePrefs` effect, and a case in
each of `handleSettingChange`'s and `handleSettingReset`'s switches, per
module — was still hand-written as of the paragraph above, so a seventh
module (`fit`, `filters`, both still deferred) would have gotten a
normal-looking control in the generated panel that changed nothing until
someone also hand-wired those four places: the same "generated but broken"
failure already fixed once, above, for `prefetch.maxConcurrent`, moved one
layer up into code only a manual browser pass could catch. Fixed by making
that state one map built from `registry.all()`, keyed by module id
(`module.ts`'s `sliceFor`), and by moving the registry itself out of
`App.tsx` into `modules/registry.ts`, so registering a module no longer
touches `App.tsx` at all. Proved with a throwaway seventh module registered
only in `modules/registry.ts`: it appeared in the generated panel and its
preference persisted across a reload with zero lines changed in `App.tsx`,
then was removed. `modules/prefetch.ts`'s doc comment, which still described
toggling `enabled` by hand-editing storage — true before Task 11's panel
shipped, false since — was corrected too. All five real modules were
re-driven in a real browser after the refactor and still switch off, tune
live, and persist, matching the table above.

**What M5b did not reach.** The filmstrip's tile burst is still exactly
where M5a left it: `prefetch`'s decode cap governs `PageView`'s page
decodes through `swapQueue`/`prefetchQueue`, and `Filmstrip.tsx` never
receives either — it renders thumbnails as plain `<img loading="lazy">`, so
a cold strip still fires one request per distinct tile, and the
`loading="lazy"` finding above still stands, unaddressed, confirmed again
rather than assumed carried over. Continuous-scroll mode **still** does not
write reading position back — entering the mode restores where you were,
but hand-scrolling never updates the stored slot, unchanged from M5a and
needing the same `IntersectionObserver` that milestone already named. The
`fit` and `filters` modules the design specifies are not built; the contract
they would slot into is what M5b spent five modules proving out instead.

**M5d has landed.** A reader can now point at the right counterpart in the
filmstrip and have the fix written into `taiyaku.json` as `by: "hand"`,
surviving a re-solve and a second device writing at the same time. The
arithmetic is one pure function in `taiyaku-core` (`correct_runs`) and one
document operation built on it (`Sidecar::correct`), so no second
implementation of run-splitting exists to disagree with `compose`. The
browser sends the *assertion* — slot `a` of `from` pairs with slot `b` of
`to` — never a computed run list, so the split is applied once, server-side,
against the authoritative document: a deliberate departure from the design's
"the client computes a run split, `PATCH` to serve", made because a
client-computed list would put the 1:1 eviction rule in a second
implementation a future reader could get wrong, and would leave `If-Match` as
the only thing standing between a stale client and a wholesale replacement of
the edge. `taiyaku-serve` gained its first mutating route, `PATCH
/api/works/{work}/alignments`, guarded by `If-Match` against an `ETag` on the
sidecar, which forced the in-memory document to become replaceable
(`RwLock<Arc<Sidecar>>`) rather than loaded once at startup; `GET
/api/works/{work}` now carries an `ETag` too. O13 closes in the same branch
(above), because the correction UI is what first constructs a `Source::Hand`.
**Driving the UI against the real corpus found one defect the whole review
chain had missed.** A correction silently stripped every `margin` on the edge
it touched — `entries_from_runs` writes `margin: None` for every entry it
builds, having no confidence figure to hand them, and the correction sent the
whole edge through it. One correction took `raw-mono -> en-fan` from 8
margins to 0 while the untouched `raw-mono -> en-mono` kept all 3. Fixed: a
margin now carries forward only where both spans are identical, and the two
runs a correction actually reshapes lose theirs honestly rather than keeping
a stale figure. See `notes.md` for the full measurement. **The same pass
found that two hand corrections are enough to take a sound alignment to
`fragmented`**, opened as O23 above rather than answered on a guess.

**A defect that manual pass reported was not M5d's, and shipped on its own
commit after it: an arrow moved by slot where a group is what is drawn.** A
joined spread is two slots and one wide image, so stepping by slot redrew what
was already on screen and the press appeared to do nothing — *"I have to right
arrow twice to go by it"* — and in `two` mode a press advanced the pair by one,
redrawing half of it. `stepToGroup` (`web/src/pairing.ts`, pure, ten tests)
steps by group instead, measured on `en-mono`, which has spreads at pages 45,
46 and 49: pressing right from displayed slot 45 walks `45 → 46 → 47 → 49 →
51`, one image per press, symmetric backwards. **Landing on the group's first
slot fixes a second thing the report did not name** — both halves of a joined
spread draw the same image but `resolve_slot` answers them *differently*, so a
reader who could not see which half they stood on got one of two different
answers on a swap from a screen that looked identical either way. **What it
costs:** the second half of a joined spread is no longer reachable by arrow,
nor is its own swap answer; it stays addressable where addressing it is the
point — the correction filmstrip puts one tile per slot, and `two` mode draws
the halves in separate cells. Confirmed acceptable during the same manual
pass. *That cost is unchanged by O28, which closed the clamp this same commit
introduced — see its entry above; what changed is only what an arrow does where
there is no next group to land in.* The grouping reaches `applyIntent` as an optional parameter with the
slot arithmetic kept as the fallback, because how the page is grouped right
now — mode, direction override, hand-inserted parity breaks — is something
nothing below the component tree knows.

**M5c has landed, and with it the thing the sidecar was for.** A note written
on `raw-mono` is readable while reading `en-mono` or `en-fan` — resolved
through the same `Resolver.resolve` the swap makes, so no second
implementation of travel exists to disagree with the first. Notes live in
`<stem>.notes.json` beside the sidecar, the placement `progress` established,
written whole under `If-Match` through the atomic writer O2 introduced; the
`GET` synthesises an empty document where no file exists, so a client's *first*
write is an ordinary conditional write rather than a second precondition
spelling. A note is anchored to a **slot**, never a page, while its region —
when it has one — is fractions of the **page that slot is drawn on**, so a
re-scan or a different served resolution cannot move a marker. *That last figure
read "fractions of that slot" until 2026-08-08; the code has measured against
the drawn page since M5c, and the two differ only on a joined spread, whose two
slots share one wide image. `notes.ts`'s `Region` carries the correction and
what it costs L1.*

**What it deliberately does not do, stated rather than discovered.** A
region does **not** travel, so a travelled note draws a margin tab and never a
tint, because drawing the untransformed box would put it over the wrong panel.
*Until 2026-08-08 the reason given here was that transforming one between
editions is L1 geometry. L1 shipped; the transform exists and is measured, and
what is missing is drawing a region through it — deliberately out of that
milestone's scope, and cheaper now rather than free, since the transform is
shared and the drawing is not. See §5.* Two devices annotating
one work do not *merge* — the second is refused with the current validator and
re-applies, the same bargain `patch_alignments` makes, and merging would need
this server to decide what two conflicting edits to one note mean. Export is the stored
document rather than a rendering, so it can be put back.
**Markers work in all three reading modes**, including continuous scroll,
where each mounted group carries its own overlay and every slot's notes are
resolved once for the whole edition rather than once per page.

**Measured against the corpus rather than asserted.** Before any code, a
throwaway test resolved every slot of one edition into another and back — 193
of 193 on Go-Toubun v01 and 28 of 28 composed through a hub, with **no
disagreement in either direction** — which is what licensed resolving once per
edition rather than once per note, and so kept `Binding`'s route memo (O14)
useful. In the browser, a note on `raw-mono` slot 88 drew its travelled tab on
the page `taiyaku lookup` independently names, in both other editions; a
dragged region round-tripped to within 1e-5; a note on `raw-mono` slot 1 — a
real gap — was listed as unreachable rather than dropped; and a second writer
racing the reader was refused, re-read, and lost nothing. See `notes.md`,
"M5c: a note written on the raw, read on the English". **One defect turned up
in that pass and it was older than the branch**, now O25: no panel below the
gap branch was ever rendered, so the review list could not be opened from the
one state it was written for. The list was fixed there; the settings panel and
the chrome were not, and the item said why. *O25 has since closed — see its
entry above. This paragraph describes the state at the end of M5c, which is
what it is here to record.*

**Two more came from the owner opening the reader, and both are worth stating
because a nine-item browser pass had already passed over them.** Typing a note
fired every reader key — `isInteractiveTarget` knew about a button, a link and
an input, and nothing about a `textarea`, because until M5c the reader had
none — so `s` opened the settings panel mid-word, Space swapped edition and a
digit jumped to one. And drawing a region worked while showing nothing at all:
the gesture recorded correctly, but no rectangle was drawn during the drag, and
an unsaved draft is in no note list for `reveal` to tint. Both are fixed, and
both were missed for the same reason — the pass produced state the shortest
way (`fill()`, dispatched pointer events) rather than the way a person does, so
it confirmed the arithmetic and never the gesture. A third followed: the
overlay was positioned on the **page area** rather than on the page, so every
marker was offset by the letterbox around a portrait page — the drag
arithmetic had been right all along and the drawing wrong, which is why every
check that read the stored fractions back agreed with itself. Markers are now
placed by an overlay that measures the page it sits over, which also gave
continuous-scroll mode the markers it had never had. *That fix stopped one
layer short, and only `fit` could reveal it: the overlay measured the `<img>`
**element**, and until `upscaleToFit` shipped on 2026-08-08 the element and the
artwork inside it were the same box. §6 has the 376px that opened up between
them, and where the measurement moved to.* **All four share one
cause:** each was invisible to a check comparing this code's output to its own
other output, and obvious to one comparing it to what a person meets — the
pointer's coordinates, the image's rect, a real keystroke. See `notes.md`,
"M5c, reported from a real browser" and "The annotation overlay was placed on
the page area, not on the page".

What remains: the eight open items tabled above — seven of them found by a
whole-branch review, now that O1 (found by one, closed by removing the round
trip its own entry was arguing about), O12 (carried rather than found), O14
(found by one, closed by M5b), O26 (found by one, closed on its own branch),
O28 (found by walking the reader through every position it can hold, closed on
its own branch), O29 (found by driving O26's own fix against the corpus, closed
the same way), O35, O36 and O37 (found by reading `web/src` closely enough to
write tests for it, and closed on that same branch), O38 (found by a probe
beside the survivors, closed on a branch of its own with the corpus drive its
entry asked for) and O39 (found beside the same survivors, closed by a
per-test `slow-timeout` and a job-level `timeout-minutes`, both sized from a
measurement) and O34 (turned up by asking a diagnosis about the one case it
could not explain, closed by L2b Task 6 counting ink pixels) are all out of the
count.
One is the exception: O23, turned up by measuring what a new surface produced
against the corpus rather than by reviewing a branch.
**Every milestone M5 named has now shipped.**

*This paragraph said "the five open items" until 2026-08-08, having missed
O31, O32 and O33 when L2a landed them and O34 when the detector measurement
opened it — the exact drift the open-items ritual warns the count word is
prone to. It went to "twelve" when the coverage gate opened O35, O36 and O37,
and stayed at twelve after that same branch closed all three — the third time
the word has drifted, and this time it was caught by a reviewer counting the
table's rows against its own preamble rather than by the ritual.*

**Four of the nine wait on evidence rather than on code**, which is worth
stating plainly now that the one item with a code-shaped exit has taken it:
O6 needs an art book with a legitimate reason to be in a sidecar,
O9 a client issuing concurrent requests for one key, O11 archives replaced
often enough for the cache to matter, and O23 a work corrected enough times
in real reading to say whether the verdict misleads. O24 is the odd one —
not evidence but a decision, about what a merge owes an edge it did not
solve, and no corpus document has yet had a `--direct` edge and a re-solve
in it.

**Two came from the mutation branch and are unrelated to each other**: O38, a
real defect in `unpaired_runs` that no test covered because the branch that
found it was a test-only branch, and O39, the absence of any bound on a test's
runtime. Both are recorded above.
*O38 has since closed — see its entry above. O39 has since closed too — see its entry above.*

**The remaining four are the detector's**, and they are held together rather
than picked off: O31 (the axis is per page where the design says per cluster),
O32 (the `median_area` disqualifier cannot fire), O33 (two constants reject
nothing in any test) and O34 (`MIN_INK` rejects nothing on the corpus either).
Each would sharpen an instrument that is measuring the wrong population, so
none is worth fixing before the detector is redesigned — and what that
redesign has to account for is now measured rather than assumed: see the two
mechanism corrections under "What is known to be wrong or missing".
*The redesign has since happened, and it closed one of the four: O34, by
counting ink pixels. The other three are still open and still held together —
O32's own fix is unwritten (the decision it was blocking was made under a rule
that never consults `median_area`, which narrows it without closing it), the gap
O33 names is in the tests, and O31's per-page axis was replaced by a two-axis
union on the L2b branch. **O31's entry has since been rewritten against that
change rather than left describing deleted code** (2026-08-12): all three of
its consequences are addressed, and it stays open on the residue the union
introduced — a block both smears produce identically has no axis to discover
and takes the dedupe tie-break's default, which is 13 of 69 blocks over the
90-page sample. See each entry above.*

**Three more were the reader's, and they are closed rather than open** — O35,
O36 and O37 were each a decision plus a few lines, and the branch that found
them made the decision and wrote the lines rather than filing them: `178f3e5`,
`4303dee` and `f4047b7`, plus `7cab6ec` for the second copy of O37's wrong
claim that fixing the first one turned up. They were the first items in a while
that could be closed in an afternoon, and they were. The argument for filing
them instead was a good one and is kept under "O35 to O37 came from the coverage
branch" above, together with what decided it the other way.

### 4. Skipping scanlation credit pages — shipped

A reader setting that keeps donation pleas, credits cards and recruitment ads out
of the reading flow, and puts them in a section of their own that can be opened
deliberately. Not deleted and not hidden — a scanlation group's credit is the
thing it asked for in exchange for the work, and a reader that silently discarded
it would be taking a side. It is simply not part of reading the chapter.

**The evidence says taiyaku can already find them**, which is why this was on the
roadmap rather than in the "if it is worth it" pile. Measured on the primary
volume — see `notes.md`, "Scanlation credit pages are already visible in the
alignment" — **four** of the fan edition's seven chapter files each contribute
exactly one page that pairs with nothing and sits at the member's final index:
`c001`, `c003`, `c004` and `c005`, at pages 95, 168, 198 and 225. Three are the
same `lhtranslation` donation card; the fourth, page 225, is a **staff-credits
page** naming three people, not the donation card. *This paragraph said "three"
and "all three are the same donation card" until 2026-08-07, which is the exact
mislabelling `notes.md` corrected on 2026-08-05 — the branch fixed the working
log and left the living document standing wrong. Both now say four.*

**Long unpaired runs are usually genuine content the other edition lacks — but
not always, and a bad alignment is the exception.** On Go-Toubun the 42-page run
really is the prototype one-shot the raw volume has no chapter for. On
`kaguya-v01`, whose sidecar the solver itself rates `FRAGMENTED` (average run
13.3 slots, 23 runs), the runs at pages 105–111 and 113–118 are **ordinary
manga** — `c5.5` has fifteen pages and the solver paired exactly two of them, so
the runs either side are an artefact of the alignment rather than a fact about
the release. Length therefore says what it says only as far as the alignment
under it is sound. **Position is the part that survives**, and it is what the
rule actually tests: an isolated unpaired page at a member's *final* index. Every
sidecar already carries the alignment that shows it.

A second signal needs no alignment at all: the cards recur across sibling members
of one edition, so a near-duplicate by dhash finds them even in an edition with no
counterpart, which is the common case for a fan release. They are byte-different
per chapter, so exact hashing does not. **It shipped as `credits::recurrences`,
a diagnostic that classifies nothing** — measured on Go-Toubun, its two tightest
matches in the whole edition are the volume cover, and it misses both
staff-credit pages entirely, because each appears exactly once. It is printed
beside every row so a person can judge a candidate by it, and consulted by no
code; that module's own doc says why gating signal 1 on it would veto the two
true positives signal 1 uniquely finds.

**All four of the questions this section used to leave open are settled, and
three of them are settled in code.**

- **Where the classification lives.** In `taiyaku.json`, as a per-edition
  `credits` list of page indices — the way `spreads` is, and for the reason both
  had in common. Specified in `sidecar.md`; carried by `Edition::credits` in
  `taiyaku-core::sidecar`, and carried across a merge or dropped with a replaced
  spoke by `merge_spoke`.
- **Who records it.** Not `align`. A command of its own, `taiyaku credits`, which
  **proposes and prints and writes nothing without `--write`** — the discipline
  `inventory` and `duplicates` already set. The rule itself is
  `credits::credit_pages` over `taiyaku_core::unpaired_runs`.
- **What the toggle does to page numbering.** Nothing. **Skipping never
  renumbers**: a slot keeps its number whether or not it is skipped, so the
  readout jumps rather than counting differently. The slot is the addressed unit
  and `sidecar.md` is normative on that, so a per-reader toggle that changed what
  an address meant would make one device's progress and another's notes wrong.
  Same precedent as the swap: what is shown changes, what is written does not.
  Recorded on `web/src/modules/credits.ts`'s `creditsModule`, and carried out by
  `pairing.ts`'s `groups`, which is handed the skipped slots and leaves them out
  of the grouping — so the page area, the arrow keys and the filmstrip all skip
  together, and `Group.slots` still indexes the unfiltered table.
  *This said "carried out by `nextReadable`, which walks to the next unskipped
  slot" until 2026-08-07. That function was written three commits after an arrow
  stopped stepping by slot and started stepping by group (#12), so it could not
  have been wired without undoing that; and it governed arrows alone, leaving a
  skipped page in the filmstrip and in scroll mode. It is deleted. A first
  attempt filtered finished groups instead, which turned out to do nothing at
  all in two-page mode — see `notes.md`, "The credit-pages panel, and where
  skipping had to be decided", for the 122-tiles-either-way measurement that
  moved the decision inside `groups`.*
- **The false-positive rate against colour inserts and author afterwords.**
  Measured, and the answer is not the comfortable one: a colour insert *is* swept
  up. `kaguya-v01`'s one candidate is the publisher's own colour announcement
  page. See `notes.md`, "A second group's habits, and what the bound is worth".

**Precision is measured, and it is not perfect: four true positives and one false
positive across the corpus.** The four are Go-Toubun's pages 95, 168, 198 and 225;
the one is Kaguya's page 212. Both are pinned by gated corpus checks
(`taiyaku-cli/src/corpus.rs`), which assert the classified *set* rather than its
size — four of six is reachable by classifying the wrong four — and the Kaguya
check pins the false positive deliberately, so that a later improvement which
stops classifying 212 goes red and the expectation is updated rather than the
rule reverted. **Recall is worse than precision and is bounded above rather than
known:** `c000`'s page 43 is a credits page for a second group that the solver
*paired*, so no unpaired-based signal can reach it at all — four of five at best,
out of twelve pages anybody has looked at. `MAX_CREDIT_RUN` is **1, measured**
over 461 pages and four scanlation groups, and one of those four groups prints no
credit page at all. Two volumes are not the population.

**The reader's half has now shipped too, and both toggles govern something.**
`C` opens a panel listing the viewed edition's credit pages — a thumbnail and a
page number each, and no labels, because the sidecar records page indices and
which one is a donation card is something a person establishes by looking. An
edition with none says so in words. Taking an entry goes there whether or not
`skip` is on: nothing here is meant to become unreachable, and the arrow that
follows steps off again.

**`skip` is honoured inside `groups` rather than by filtering its result**, and
that was not the first design. Filtering finished groups can only drop a group
whose every slot is skipped, and measured on this volume that is none of them —
each of the four credit pages sits at a chapter's end and so pairs with an
ordinary page, giving 122 tiles in two-page mode with the setting on and 122
with it off. Deciding while pairing lets a skipped slot's neighbour pair
forward instead. See `notes.md`, "The credit-pages panel, and where skipping had
to be decided".

`creditSlots` is gone with it, replaced by `creditSlotsIn`, which filters the
slot table `allSlots` already returns rather than re-deriving slot expansion in
TypeScript — one derivation, read through the `Resolver` from
`taiyaku_core::slot::expand`, which is what its own doc comment asked for. The
`Pagination` it could not be given no longer exists.

### 5. L1 geometry — shipped, and what shipped was in neither candidate list

Promoted out of "Later" on 2026-08-08 rather than reached in order, which is why
a list that opens "all four" has a fifth entry.

**What it is.** Free continuous zoom and pan in single-page and two-page mode —
wheel or pinch anchored at the pointer, a drag to pan — a page turn that carries
the view onto the next group and re-clamps it, and a swap that carries the
*visible content*: the rect the reader is looking at, named against the other
edition's page and then fitted around. Zoom is a `ReaderModule` like the other
eight, so its four preferences generate their own settings section with no
hand-wiring and its three keys are checked against the live registry at startup.

**A drag pans from the grey around the page as well as from the page.** That
took two goes. The first rule was "the press is on the drawn `<img>`", which
was the narrowest thing true of the page and false of a control — and it made
the letterbox dead to a drag, which a reader trying it reported straight away.
The comment defending it had *stated* that cost and judged it acceptable on the
grounds that the letterbox shrinks as the zoom grows; what that missed is that
a reader reaches for the nearest grey to drag from, and being ignored there
reads as the pan being broken rather than as a boundary. It now excludes
controls instead of requiring the page, via `keys.ts`'s `isInteractivePress` —
the ancestor-aware sibling of the `isInteractiveTarget` the keydown handler
uses, and the ancestry is the point: a press on a note-list entry lands on the
*text*, so the keydown rule answers `false` for exactly the case the image test
existed to catch. Confirmed in the browser both ways: a drag from the grey pans,
a drag begun on a note's margin tab does not.

**Verified in a browser against the corpus**, on sidecar copies with absolute
member paths so nothing written landed in `corpus/`: a wheel zoom holds the
point under the pointer to zero drift and the scale is uniform at every
magnification sampled; a swap onto a joined spread lands on the correct half
(`raw-mono` 5 → `en-mono` 4 at page fraction x 0.526–0.974, which `taiyaku
lookup` independently calls "4 right half"); two-page mode carries a viewport
straddling both cells; an annotation region stays on its art through a zoom and
a pan; and scroll mode draws no transform at all. What no test here touches is
trackpad *feel* — `PER_NOTCH.pinch` and the notch caps are guesses, and a
synthetic wheel event cannot say whether `{ passive: false }` defeats a real
trackpad's chrome-zoom.

**Fit beat fill, and it is the swap's whole promise.** Filling the viewport
crops a sliver that was visible a moment ago, and a swap that hides something
the reader was looking at is the failure that makes the feature untrustworthy.
Fitting shows a little extra instead — the same conservative direction the
composition loss takes, where a reader sees "no counterpart" rather than a wrong
page. Magnification is deliberately *not* the invariant: two editions are
different physical printings at different scan resolutions, so "the same zoom
factor" is a statement about pixels rather than about art.

**Carrying the view across a page turn beat two alternatives.** Panning to the
reading-order start is the conventional reader behaviour and is wrong here for
the same reason the swap preserves content — someone zoomed into a corner is
working through something, and moving them is a decision made on their behalf.
Resetting on every turn has no hidden state and makes a reader with small text
re-zoom on every page, at a cost borne entirely by the person the feature exists
for.

**Nothing in the sidecar, the aligner or the server changed**, and that was the
scope decision rather than a happy accident. The transform is derived at read
time from the two drawn images' natural dimensions; `sidecar.md`'s
forward-compatibility rule already permits adding a stored one later without a
version bump, so the door stays open at no cost — which is the reason not to
have walked through it on no evidence, the same ground the gatefold question is
deferred on.

**The measurement went against the design, and that is the headline.** The
design named two candidate transforms and a baseline, and fixed the promotion
rule before any number existed so that the result decided the cost rather than
the cost deciding how the result reads. `crates/taiyaku-cli/src/geometry_experiment.rs`
then sampled 288 paired slots — 24 each from twelve corpus volumes across four
series, `raw-mono` against `en-mono`, the one edition pair all twelve carry —
and for each ran a coarse-to-fine grid search for the best rect-to-rect map,
scored with the solver's own `similarity` over `fingerprint` rather than a
metric invented for the question, so the numbers are in the same currency as
every other figure in `notes.md`. Reported over the 284 whose search settled;
the four dropped ended the coarse pass against the wall of its own range, which
is the search saying it ran out of room rather than that it found a minimum.
The outcome:

- **`pillarbox` won, and neither the design nor the plan had named it** — the
  experiment is where it came from, because nothing had looked yet. It takes
  the **min** of the two aspects as the shared content's where `letterbox`
  takes the max — one token in `mapRect`. Median residual 0.0195 of page width
  and 0.0041 of page height, p90 0.0276 and 0.0116; it beats `identity` on the
  horizontal in 10 volumes of 12, and the two it does not are the two whose
  aspects nearly match already, where there is almost nothing for either rule
  to correct.
- **`identity` finished second**, 0.0296 / 0.0041 — exactly right on the
  vertical, uncorrected on the horizontal.
- **`letterbox`, which the design named and which `mapRect` carried from the
  task that built it until the measurement replaced it, finished third and lost
  to doing nothing.** 0.0296 / 0.0162: it never rescales
  the horizontal, so it inherits `identity`'s horizontal residual whole, and
  then adds a vertical error four times `identity`'s at the median on an axis
  the pixels wanted left alone. A candidate that can only add error to one axis
  and correct nothing on the other is not a candidate.
- **`reset` loses to all three.** It is a bound rather than a residual — the
  reader sees the whole page and re-finds the balloon, so up to half of one —
  and it does not need measuring while every candidate above is inside 5% of a
  page.

**The horizontal finding could have been an artefact, and two runs exclude it
between them.** `fingerprint` resizes every crop to one fixed grid, so a search
can buy similarity by squaring two renderings up rather than by locating
artwork, and the finding — the best map's horizontal scale tracking `aFrom/aTo`
in twelve volumes of twelve — is exactly what that would look like. A control
ran the same search against a resample of the *source* page, where the aspect
difference is fully present and the true answer is known to be identity: it came
back `sx/sy = 1.0000` in all twelve, at a residual of 0.0011. And the same
search run backwards over the same slots inverts, in twelve of twelve — the two
directions' horizontal scales multiply to a median 0.9941, against the 1.0842
they would give if the scale belonged to the metric rather than to the pages.
Neither run alone would do it: the control excludes the direction-antisymmetric
artefact and the reverse run excludes the direction-blind one.

**The threshold this was judged against is still a guess, and says so.** What
fraction of a page a panel occupies has never been measured and this experiment
did not measure it either; the working figure was that 5% of a 1200px page is
about 60px, a fraction of a balloon rather than a panel. `pillarbox`'s p90 of
2.8% of page width sits at half that and its vertical p90 of 1.2% at a quarter.

**The spread half was a scope gap, and it was closed on 2026-08-08.** Where one
edition joins a spread and the other splits it, a swap now lands on the half the
reader's slot names. Both halves of a joined spread are drawn as one wide image
and share one coordinate space (`notes.ts`'s `Region`), so getting a reader to
the correct half is the transform's job rather than something the slot model
absorbs — which is what the design's §1 said and what the implementation did not
do. The fix is two arguments and no new arithmetic: `viewport.ts`'s `Drawn`
carries a `half`, `mapView` takes the rect out of that half's box on the way in
and puts it back on the way out, and `mapRect` is handed the half's own frame
rather than the whole spread's, since it reconciles two *aspects* and a joined
spread's is twice its half's. Where the half sits in the composition is forced
rather than chosen — a half is half of the *image*, and `paintedIn`'s letterbox
is where image fractions begin, so a half applied before it would be half of the
cell instead. A slot with no half passes through both new steps as a
floating-point identity rather than a near one, which is why they are
unconditional and why every `mapView` test written before them holds unchanged.

*Until that date this paragraph read "what it does not do, stated plainly
because a reader would hit it". `mapRect` was handed the two drawn images and
nothing about halves, so it read the spread's extra width as margin at both
edges and mapped its central band onto the single page — the fold, rather than
either of the pages the other edition prints. The finding was **read off
`mapView`'s composition rather than driven**: the by-hand pass crossed a joined
spread with an *arrow*, which is a turn within one edition and unaffected, and
did not swap between an edition that joins one and an edition that splits it.
That is the part worth keeping now the defect is gone. It was reachable on the
primary corpus — `en-mono` joins Go-Toubun v01's page 4 where the raw prints two
pages — and a pass with hands on the keys still did not meet it, because meeting
it needed one particular key pressed in one particular place.*

**Two smaller exposures remain, and both are measured rather than guessed.**
`pillarbox` leaves 2.0% of page width at the median, part of which is a per-page
horizontal offset that moves by up to 0.083 within a single volume and which
nothing derived from two frames can correct; and the vertical residual is 0.4%
at the median against a control floor of 0.1%, uncorrected by every candidate
here.

**Travelled note regions are still not drawn**, and the reason has changed
rather than gone: the transform they needed now exists, so what remains is
drawing through it. L1's design put that out of scope by name to keep the
milestone one shippable thing.

### 6. Fit, the seam, and the keybind help — shipped, and two of its designs were wrong

Three unrelated reader complaints raised after L1 shipped, grouped onto one
branch because each is small and none touches the alignment. They are not one
feature and this entry does not pretend otherwise.

**What it is.** A `fit` `ReaderModule` — `upscaleToFit` (on), `upscaleFilter`
and `gap` — so a page fills its box instead of sitting at natural size in a
large letterbox; a two-page seam the artwork actually meets across, with the
note tabs moved out of the way of it; and a `help` module whose overlay lists
every key the reader answers, generated from `keys.ts`'s one ordered
`READER_KEYS` table and from the registry. Nothing in the sidecar, the aligner
or the server changed, and no dependency was added.

**The default reverses the M5 design, deliberately, and that document is
corrected in place rather than rewritten.** It set `upscaleToFit` off and
argued it well: enlarging a low-resolution scan costs something real, so the
reader should ask for it. Measured in a browser at a 1400×1263 page area on
`corpus/gotoubun-v01`, `en-mono` (2147×3056) drew at 887×1263 and filled the
height while `raw-mono` (685×1024) drew at 685×1024, leaving 239px unused — and
the first reader to meet that reported it as a defect in the images or the
aligner, not as a setting they had not found. A default that makes a working
reader look broken is the wrong default however sound the reasoning behind it,
so the default reversed and the reasoning stayed. The enlargement is 23% of
inked line work, which is why `upscaleFilter` shipped in the same commit rather
than later. Neither the aspect difference (raw 0.6689 against 0.7026, about 5%)
nor raw-versus-English explains the complaint — `en-fan` at 1114×1600 does the
same thing on a tall enough display — and both were checked before anything was
built.

**The seam's design closed the gap on the wrong element, and only a measurement
said so.** The design had each cell justify its page toward the seam. Built
exactly as specified it moved nothing: the two `<img>` *elements* already
touched at 0px while their artwork sat **359px apart**, because `upscaleToFit`
had just made each element fill its whole cell — leaving `justify-content`
nothing to distribute — and `object-fit: contain` then centres the art inside
that element. The fix is `object-position` on the image, which took the 359px to
**0px**, measured the same way; the cell keeps only the padding that splits
`gap`. **No test could have caught the dead version.** `justify-content` on a
box with a single full-width child is silently inert, so an assertion can only
see that the declaration is present, which is exactly what the assertion in
`fit.test.ts` did.

**This branch also introduced a regression and then fixed it, which is worth
the record.** Making the element larger than the artwork inside it broke every
annotation marker, because `Annotations.tsx` measured the element: in single
mode a 1385×475 element holding 334×475 of artwork, so a region tint that
belonged at x 842 was drawn at 466 — out by 376px, on the default settings, from
the moment `fit` shipped. `notes.ts`'s `paintedBox` was wrong the subtler way:
it had been an identity for the whole of its life (with `auto` sizing, box and
content coincide), and when the distinction finally arrived it split the
letterbox evenly — right for a centred page, wrong for a page against the seam.
In two-page mode it answered 179 and 872 across the page area where the artwork
was at 359 and 693. Both now measure the artwork, through one answer in
`App.tsx` that the drawn pages, the marker overlay and the drag arithmetic all
ask.

***And a fourth consumer did not ask.*** This paragraph closed "so what is
drawn and what measures it cannot disagree" until 2026-08-08, when a review of
the merged branch found `viewport.ts`'s `paintedIn` — which its own doc calls
the fractional twin of `paintedBox` — had never been given the hug. `mapView`
calls it for both sides of a swap, so a zoomed swap in two-page mode measured
both pages as centred while both were hugged to the seam. Fixed on the branch
after; `Drawn` carries the hug now. Single and scroll mode were never affected,
because their hug is `centre` and the two answers coincide there — which is
precisely why three rounds of review and a browser pass all missed it.

**The tab move is the M5 design's own guarantee going out from under it.** That
document wrote "fit-whole guarantees the side margins the tabs live in on every
portrait page", which was true of a centred page and stopped being true of a
seam-aligned one. In `two` mode a tab now sits in the margin *away* from the
seam; single and scroll mode are unchanged, and the default is the old
behaviour. Letting the tabs overlap was cheapest and is the one thing this
codebase must not do — a tab drawn over the facing page's art asserts a note is
on a page it is not on.

**The help is generated, which is the whole of its design.** `Space`, the
arrows, `Tab`, `c` and `1`–`9` used to live in `intentOf` as bare `if`
statements that nothing could enumerate, so a written help screen would have
been a second copy of a rule — and this project has repeatedly spent review rounds on
comments that were true when written and false later. One ordered table is now
dispatched from and listed from. `KeyBinding` gained an optional `label` for it,
which is a widening of the module contract and was reported rather than done
quietly, per `module.ts`'s own rule.

**Verified in a browser against the corpus**, on a scratch sidecar with absolute
member paths so nothing landed in the gitignored `corpus/`: the artwork seam at
0px in two-page mode; a region tint at x 842 where 842 was expected in single
mode, against 466 before the fix, and 663 where 663 was expected in two-page; a
note tab at x 552–570 with the facing page's artwork starting at 574; `?` and
`h` both opening the overlay and `Escape` closing it; and the ZOOM section
disappearing from the list with `zoom.enabled` off. See `notes.md`, "Fit, the
seam, and the keybind help".

**What is not built**, and why it is a deferral rather than an omission: `trim`
and `fillWidth`/`fillHeight`, which the M5 design lists in the same module.
Nobody asked for them, and `trim`'s case — Tongari v09's letterboxed scans — has
no reader waiting on it. The reasoning is kept in the M5 design for whoever hits
it.

**One known hole, stated because a reader could meet it.** A module is listed in
the overlay unless it declares `enabled: false`, and `credits` gates its key on
a preference called `panel` instead. Turning that off leaves `C` listed while
the key does nothing. It is the only module in today's registry with an off
switch under another name; the fix if it grows is a field on the contract, not a
list of per-module exceptions inside the help module, which is the copy of a
rule that file exists to avoid.

### 7. Reading direction, and what it did not reach — shipped

Reported as "the reading direction doesn't seem to work". It worked, and did
less than its name claims: it decided which slot of a pair was drawn on which
side, and nothing else.

**Two things it did not reach.** The filmstrip ran left-to-right under a
right-to-left work — measured on `gotoubun-v01`, whose raw is `rtl`: tiles ran
page 1 at x 26 through page 10 at x 939, *unchanged* when the direction was
flipped, so a reader hunting a page had to read the strip backwards from the
pages beside it. And the right arrow always advanced, so in a right-to-left
work it moved away from the page about to be read.

**One rule, three consumers.** `stepFor` in `modules/direction.ts` answers
which way a physical arrow turns the page. `App.tsx` maps the arrow intent
through it, `Filmstrip` lays its rail out with `row-reverse` from the same
answer, and `helpGroups` asks it which arrow advances so the overlay stops
telling a manga reader that `→` is the next page. Deriving the help rather
than restating it is the point: three places each deciding for themselves is
how they came to disagree.

`row-reverse` rather than reversing the tile array, deliberately. `stepToGroup`,
the current-tile border and the correction picker all address tiles **by
index**, and reversing the data would have made every one of them wrong in a
way no type could catch.

**Verified in a browser** on `gotoubun-v01` with the raw set to `rtl`: `→` went
page 2 → 1 and two `←` went 1 → 3; the strip came back `row-reverse` with tiles
descending left to right and the current tile still scrolled into view; the
overlay read `→ previous page` and `← next page`.

**The mode is remembered too**, which is unrelated except in being a thing the
reader forgot. `mode` was a `useState`, reset to single page on every load.
`layoutModule` holds it now, and registration is the whole fix — `App.tsx`
builds its preference state from `registry.all()` and writes every slice
through `prefs.ts`, so loading, saving and a settings control all follow.

It is a module rather than a direct `loadPrefs('mode', …)` call, which was
shorter and would have been the first value this reader stored without being a
module — and so the first stored key nobody thought to namespace. Everything
in `localStorage` was already under `taiyaku.prefs.*`; what was missing was
anything stopping the next value from not being, and `prefs.ts` now states that
rule where the keys are built.

### 8. Two-page mode: halves that arrive together, and a pairing that agrees across editions — shipped

Two complaints from real reading, both about `two` mode, and both the same kind
of defect: a rule that was right about one page and had never been asked about
two. `notes.md`, "Two pages that arrive together, and a pairing that agrees
across editions", is the record.

**The halves presented one at a time, and the code said so.** `decode.ts`'s gate
is per image, so each cell showed its page the moment its own decode settled.
`App.tsx` carried a comment stating exactly that, arguing for leaving it — "a
mismatch nothing has measured a reader noticing" — and naming a measurement as
the thing that should overturn it. **A reader noticed, and the measurement
agreed with them**: with a warm cache on the production bundle, a median 89.7 ms
between the two cells at a slow reading pace, 158.4 ms at worst, and **every one
of eight turns tearing by more than a frame**; flicking took the median to
213.8 ms and the worst case to 1.28 s. `cohort.ts` is a barrier that holds the
first decode until the other settles, with the decode queue nested *inside* it
so the concurrency cap still governs the decodes and the barrier governs only
when they are shown. **Membership closes on a tick rather than a count**, since
nothing knows in advance how many images a turn will change; a turn that changes
one cell forms a cohort of one. Re-measured the same way: **0.0 ms, on every
turn**.

**The "off by 1" between editions was not the aligner.** That was checked first
and comes back clean: a shift test over every paired slot of 12 alignments
across 9 volumes and 4 series — score the pairing the solver chose against the
same slot's score against `to ± 1` — finds **zero improvable pairings**. Its
first version reported four, all at volume tails with large margins, and every
one dissolved on adding the one-to-one constraint the scan itself had been
missing: several near-identical end pages match one `to` page, the solver gives
it to the highest scorer, and the loser then looks improvable against a slot
already taken.

**It was `pairing.ts`.** `groups` anchors `two` mode's pairing at slot 0 of
whichever edition it is handed, and nothing carried that across a swap — so the
same reading position drew `(n-1, n)` in one edition and `(n, n+1)` in the next,
one page right and the other its neighbour. Over every ordered pair of
`gotoubun-v01`'s three editions, **206 of 580 swaps kept the pair intact**, and
between the two English editions — the pair the complaint names — roughly seven
groups in ten were torn. Intermittently, which is why it read as "some pages
pretty early on" rather than a constant offset. `alignedPairs.ts` derives a
grouping from the alignment and feeds it in as more of `groups`' existing
`breaks`, so the grouping itself needed no new mechanism. **The hub and not the
authority** as the space it is derived in, because `authority` follows wherever
the reader last navigated and would re-group every other edition when they
turned a page.

**The first version of that rule shipped, and the reader broke it in one press
— which is the part worth keeping.** It derived the *spokes* from the hub and
left the hub on its own parity: 572 of 598, 96%. Then "there's still some
mis-alignments, like slot 48, p46". A joined spread is one image, so `groups`
gives it its own group and no break can halve it — and `en-mono` joins pages 45
and 46 back to back where the raw carries four ordinary pages. Their
counterparts are raw slots 47–48 and 49–50, but raw's untouched parity pairs
`(46,47)` and `(48,49)`, so **no raw group was the counterpart of that spread**
and the swap drew one page out either side of it. A rule with a fixed point that
cannot move is what left the 4%.

**Pair boundaries are a property of the work, not of an edition.** Wherever
*any* edition joins a spread, that is a boundary the work has; those are mapped
into the hub's slot space, unioned, and every edition — the hub included — takes
its pairing from the one grouping that results. Measured across eleven volumes
in four series it is **never worse than the spokes-only rule and strictly better
on five**: `gotoubun-v01` 96% → **99%**, `gotoubun-v08` 93% → **100%**,
`kobayashi-v08` 99% → **100%**, and 581 of 590 on the three-edition document the
complaint came from.

**A third complaint came out of the same corner, and two of its obvious causes
were wrong.** "Switching pages sometimes seems slow": median 110 ms from
keypress to both halves. `modules/prefetch.ts` reached `ahead` *slots* where an
arrow steps by *groups*, so in `two` mode the next group's second half was never
prefetched — measured on the wire as exactly one page requested after the
keypress on every forward turn. That is the same missing fetch as the tearing
above; gating the pair had turned it from a tear into a wait. `groupWindow`
reaches whole groups: **110 ms → 14 ms**. Widening the window then reintroduced
a tail, and the reason is the interesting part — a decoded page is ~26 MB, a
joined spread twice that, so a wide window evicts the pages the reader is
arriving at. **Prefetch was competing with itself**, confirmed from the other
side by raising `maxConcurrent`, which made it worse. The defaults are now one
group each way: **median 4 ms**. What survives is a tail of a few turns in
twenty at 100–300 ms, when the browser has evicted a bitmap.

**Two dead ends are recorded rather than dropped.** Retaining the prefetched
images does not retain their bitmaps — 72.5 ms with no references held against
160.4 ms holding twenty-six, since Chrome discards the raster for a detached
image either way. And progressive JPEG is the genuine decode cost — `en-mono`
ships progressive at 76.7 ms against 26.4 ms for the identical image as
baseline, where downscaling to a quarter of the pixels saves only 6 ms — but a
lossless transform was built against `turbojpeg` and **backed out**: it costs a
median 86 ms in process, more than the 50 ms it saves, so it needs a cache and a
cold-path policy or the first read of every page gets slower; it needs
`libturbojpeg` in CI; and Komga, the comparison that prompted it, passes JPEG
through untouched and pays the same decode.

**What it costs is a page drawn alone, and it is not hidden** — but it costs
less than the rule it replaced, because the hub can now move rather than every
spoke bending around it. Each position where the parity genuinely has to change
costs one solo group: 4 added to `en-mono`'s 104 on `gotoubun-v01` where the
first rule needed 14, 2 to `en-fan`'s where it needed 12, and `gotoubun-v08`'s
`en-mono` 16 → 2. A reader who never swaps pays that too. Taken deliberately: a
solo page says there is nothing here to pair this with, where a silently
mismatched pair said nothing at all.

### Later

- **The page cache used to follow the working directory. Shipped 2026-08-11**
  on `serve-cache-root`; `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md`
  is the record, and this entry is kept as the defect that opened it.
  `ServeState`'s `cache_dir` defaulted to the *relative*
  `.taiyaku-cache/covers`, so where a cache landed depended on where `taiyaku
  serve` was launched from. That was cheap while the cache held covers and
  tiles; since `?w=` it holds every reading-size page, so starting the server
  from a different directory silently produced a second cache and regenerated a
  whole volume at ~126 ms a page. Found by noticing 120 MB of pages under
  `web/` after a session that had launched `serve` from there. Gitignored, so
  nothing could be committed, and harmless beyond the wasted work.
  It now resolves to `$XDG_CACHE_HOME/taiyaku`, falling back to
  `$HOME/.cache/taiyaku` and then — printing a line that says so — to the old
  relative path, with `--cache DIR` overriding all three. **Two alternatives
  the entry itself had proposed both lost.** Resolving against the sidecar,
  the way `stump::cache::path_for` does, cannot work as stated: that function
  takes a `&Work` and runs per work, while `cache_dir` is one directory for a
  server that may be handed sidecars in several directories, so there is no
  single sidecar to resolve against. Canonicalising against the first sidecar
  would have made the location depend on argument order instead, trading one
  surprise for another. Both would also have written into the corpus tree.

- **Move `web/` from npm to pnpm. Shipped 2026-08-09** on
  `toolchain-pnpm-node26-ts7`; `docs/ci.md` is the record now, this entry is
  kept as the decision that opened it. Decided while adding the browser test
  project, and deliberately not done in the same branch: it touched the
  lockfile, `.forgejo/workflows/ci.yml`'s `npm ci`, every `npm --prefix web`
  call in `scripts/check-all.sh`, and the docs that named them — none of which
  had anything to do with the reader. It also had to be stacked rather than
  parallel, since the lockfile it produces had to already contain the browser
  dependencies.
  **Two things were flagged to check rather than assume, and both were.**
  `taiyaku-wasm` was a `file:` dependency on the gitignored, generated
  `crates/taiyaku-wasm/pkg`, and `vite.config.ts` widens `server.fs.allow` to
  exactly that directory — pnpm's symlinked layout does resolve such a
  dependency differently from npm's flat one: `file:` does not survive a
  fresh clone under pnpm at all, which is why the protocol shipped as `link:`
  instead (`docs/ci.md`, "Why `link:` and not `file:`, which is what npm
  used"). And the image running CI's `web` job had no pnpm; it ships as
  `npm i -g pnpm@11.3.0` in a step of its own rather than through `corepack`,
  which shipped with Node 22 but not enabled by default. Whether
  `node:26-bookworm` still carries it was recorded as unmeasurable — no local
  docker to check the image, per the design document's §10, "What could not
  be measured." It is not merely unmeasured: this machine's own Node 26.7.0
  install has no `corepack` on `PATH` (`command -v corepack` returns
  nothing), so the parenthetical this sentence used to carry — "and is in
  Node 26" — was wrong, not just unverified. A later branch reaching for
  `corepack enable` in place of `npm i -g pnpm@11.3.0` on the strength of
  that sentence would die on the `web` job the same way.

- **Fall back to another edition where this one has no page, marked as such.
  Shipped 2026-08-13** on `gap-fallback`;
  `docs/superpowers/specs/2026-08-12-gap-fallback-design.md` is the record, and
  this entry is kept as the opportunity that opened it.
  The reader shows `Gap` when the edition being read has no counterpart for the
  slot — an honest answer, and a missed opportunity. The alignment already
  knows which *other* editions do hold that page: `route` will answer for any
  pair, and `nearest.ts` already scans for the closest paired slot. So instead
  of a page that is simply absent, the reader could draw the counterpart from
  whichever edition has one — the raw, the fan release, a colour edition — with
  the substitution **marked** rather than silent, in the same spirit as the
  `via` field distinguishing a direct answer from a composed one.
  **This is arguably the whole point of the alignment**: a reader following a
  story should not lose a page because the edition they happen to be in dropped
  it, and no single-edition reader can offer that. It wants a setting rather
  than a default — a reader comparing translations may want to *see* that a
  page is missing — and it wants the marking designed properly, since a page
  silently drawn from another edition would be worse than a gap. Raised while
  reviewing the two-page work, where the four solo groups the aligned pairing
  produces are the same fact seen from the other side.
  **What shipped** is `web/src/modules/borrow.ts`'s `lenderFor`: candidates are
  `orderedEditions`' hub-first order minus the viewed edition and minus the
  authority, the first that resolves lends, and the authority lends when none
  of them does. It is layered onto `display` by `withBorrow` rather than built
  into it, because `nearest.ts` reads `display`'s own answer to decide whether
  an offer lands on a *real* page, and a `display` that returned `borrowed`
  would have turned every gap offer into a jump to a third edition. The mark is
  a tinted leading edge and a button naming the lender and its page, with the
  page's `alt` text saying so **unconditionally**, so the substitution is never
  a purely visual signal; and that button **is** the way back, since pressing
  it reveals the `Gap` view for that slot, offers intact, rather than spending
  a keybinding on a screen most readers meet rarely. **The revealed side
  carries a control back to the borrowed page**, which the first cut did not
  and which a whole-branch review caught: pressing the mark is looking, not
  choosing, and with the arrows inert (below) and `Gap` taking no dismiss
  handler, a reader who looked could only get the page back by swapping to
  another edition and back again. The setting the entry
  asked for exists and defaults **on** — the case it made, that a reader
  comparing translations may want to *see* a page is missing, is served by the
  switch existing rather than by its default, and a reader who never opens
  settings is better served by the page than by the gap.
  **Two findings from reading the code shaped all of it, and this entry knew
  neither.** The authority edition can always lend: `display`'s `from` is
  `resolver.slotAt(authority, slot)`, so wherever it is defined the authority
  holds that exact position by construction — which means **the only gap that
  survives the feature is the one where the authority's own slot is invalid**,
  a state `restore`'s validation and `navigate`'s resolver-derived slots
  already prevent. And a gap is a single-position state rather than a per-page
  one: `App.tsx` consults `shown.kind` *before* it consults the mode, so a gap
  replaces the whole page area in every mode and a borrowed page is therefore
  drawn **solo**. **The entry was written expecting the opposite**, and that is
  worth the same keeping as L1 geometry's wrong line below: "draw the
  counterpart from whichever edition has one" pictures a borrowed page woven
  into the sequence beside real ones, and there is no scroll column with a hole
  in it and no half-borrowed pair for one to be woven into. An early
  brainstorming sketch drew exactly that state; it is not reachable.
  **What the rule gets wrong, recorded rather than left to be discovered.**
  Preferring a third edition over the authority is right when the authority is
  the raw and the third is another English release — the case the feature
  exists for — and wrong in the mirror case, authority `en-fan`, viewed
  `en-official`, third `raw-mono`, where it draws the raw over the other
  English edition. **Nothing available distinguishes them**: the sidecar's
  `editions` block carries a free-text `label` and no language, script or kind
  field (`docs/sidecar.md`), so "prefer a same-language edition" is not
  expressible at all. A reader-set order per work is the escape hatch, deferred
  until reading produces the case rather than built against a guess.
  **Verified in a browser against `gotoubun-v01`**, the only corpus material
  with three editions and so the only place the rule does anything the rejected
  "authority always" rule would not: viewing `en-mono` at `raw-mono` slot 1
  draws `en-fan` page 44, and viewing `en-fan` at `raw-mono` slot 193 draws
  `en-mono` page 186. **The authority lent in neither direction** — which is
  exactly what preferring a third edition exists to produce, measured rather
  than assumed. The session is `notes.md`, "The lender rule in a browser: two
  directions, and the authority lending in neither", including what it does not
  establish — one volume, one session, and the mirror case the rule gets wrong
  is not reachable on any corpus material.
  **The two-edition degeneration, stated rather than hidden.** With only two
  editions there is no third to prefer, so the rule draws the page the reader
  just swapped away from, marked — and that is most of the corpus. The mark is
  what distinguishes it from the swap having done nothing; whether a reader
  finds it useful or irritating is a question only reading answers, and neither
  the design nor this entry claims to know. It carries a consequence larger
  than "adds a branch", which the design did not state: with the module on,
  **`Gap` is now reachable only through the mark**, and on a two-edition work
  it is otherwise seen only with the module switched off.
  **A borrowed page is still not a reading position.** `at` stays `undefined`
  exactly as in a gap, so annotations do not draw, zoom does not engage, and
  the page is not a prefetch target — every rule that guards on `at` is
  unchanged, which is what keeps this off the correction UI, note anchoring,
  `pairing.ts`'s grouping and the filmstrip.
  **And the arrow keys do nothing**, which is the one item on that list a
  reader meets without going looking for it. `session.ts`'s `step` calls the
  *unwrapped* `display` and refuses anything that is not `at`, so from a
  borrowed page ← and → are silently inert while an apparently ordinary page
  fills the screen. Measured in the browser rather than reasoned about:
  `ArrowLeft` left the page exactly where it was, and the design had not
  anticipated it (`notes.md`, "The lender rule in a browser: two directions,
  and the authority lending in neither").
  **That is the feature's real cost, and it is a discoverability cost rather
  than a mechanical one.** `Gap` announced its dead end in a sentence and put
  both escapes on screen beside it, so an arrow that did nothing was explained
  by the screen it did nothing on. A borrowed page announces nothing. Its only
  in-page escape is `Borrowed`'s mark, whose label reads as a caption rather
  than as a control, and a reader who does not press it has nothing on screen
  to tell them why the arrows are silent — they have been handed a page that
  looks like every other page and behaves like none of them. The trade was
  made knowingly, and the entry's own premise is that the page is worth more
  than the gap; it is still a trade rather than a free win, and any complaint
  that arrives about this feature will most likely be this one.
  That the page is not a reading position is a limitation rather than an ideal,
  and it is **deliberately a note rather than an open item or a new entry in
  this list**: no reading has produced the complaint, and how often a
  gap arises at all is itself unmeasured (the design's §10). `notes.md`,
  "A borrowed page is not a reading position, and might one day want to be",
  carries the three questions making it one would have to answer first.

- **Pre-warm a work's page sizes on load.** Since `?w=` shipped, the first read
  of a page at a given size costs the server a decode and a resize —
  **126 ms median generation against 10 ms warm**, measured over 235 real
  generations. Prefetch reaching one group ahead covers ordinary reading, since
  generation is faster than a reader turns pages; a fast flick through cold
  pages is not covered, and neither is the first page of a session.
  **Deliberately not built**, and the reason is a hazard rather than effort:
  resizing runs behind `cover.rs`'s two-permit `GENERATION_LIMIT`, shared with
  covers and tiles, so a warm pass would occupy both permits and the reader's
  *own* cold page would queue behind it — making the case it was meant to fix
  worse. Any attempt needs to answer that first: a separate low-priority permit,
  or warming only while the reader is idle. It must also fetch without decoding,
  or it re-creates the bitmap-eviction problem `pageSize.ts` exists to remove.
  Client-driven, too — the server does not know a viewport and would have to
  guess a rung or generate all six.

- **L2 text regions** — a reader points at a balloon on the raw and sees the
  matching English one. **Stays here, and the line that used to describe it was
  wrong twice.** It read:

  > **L2 text regions** — bubble pairing by transformed centroid, which L1's
  > `mapRect` now makes arithmetic rather than a matching problem.

  It is right that the pairing is arithmetic now. It is wrong that a region is a
  **bubble** — what serves both the pairing and a later text-selection pass is
  the **text block** inside the balloon, since a balloon detector is blind to
  the borderless narration manga uses constantly. And it is silent about
  **detection**, which is the larger half of the work by a wide margin: nothing
  in this repository detected anything on a page, and "pairing by transformed
  centroid" presumes a centroid on each side to pair.
  **L2a has since landed the half that is not detection** — the
  `<stem>.regions.json` format in `taiyaku-core`, the `regions` command, and
  `scripts/region-survey.sh` — and measured the half that is. The detector does
  not yet clear the design's own bar, for a reason that is now measured; see
  "What is known to be wrong or missing" above, and `notes.md`, "The text-block
  detector, measured against four works, and why it does not work yet". What
  remains before a reader sees anything is a detector whose scale is computed
  over glyphs rather than over screentone, and then L2b's pairing.
  **The instrument that can judge such a detector shipped on 2026-08-11**, as
  sub-project A of two;
  `docs/superpowers/specs/2026-08-11-l2-ground-truth-instrument-design.md` is
  the record. A `taiyaku.truth.json` format in `taiyaku-core`, `GET`/`PUT
  /truth` on the server, a browser labeller at `/label.html`, and
  `taiyaku score`, which reports **five outcomes per hand-drawn box** —
  one-to-one, merged, split, fragment, missed — plus recall per kind and the
  count of boxes flagged `ignore`. `taiyaku score --sample corpus` prints the
  worklist: 45 dev pages over nine editions, and 30 sealed held-out pages over
  six the survey has never touched.
  **Why an instrument came before another candidate**: every acceptance rule
  this project has stated for the detector has since failed *as a rule* —
  `median_area` could not fire, `blocks_per_page` ranks the editions
  backwards, and L2A Task 9's rule inherited the second one's blindness. A
  fourth would have failed the same way, and nobody would have found out until
  someone cropped boxes by hand again.
  **The first cut of the scorer reproduced that failure exactly**, which is
  worth recording rather than quietly fixing: it decided "merged" by *counting*
  how many labelled boxes a prediction covered, so a whole-page prediction over
  one balloon came back a clean hit — and three of the five panels in
  `notes.md`'s cropping table hold exactly one balloon. Classification is
  directional now; association is not. The line that stood here read
  "**Nothing is labelled yet**, so no figure about the detector has changed and
  sub-project B has not begun", and it was true when written and is no longer:
  478 boxes over nine editions are labelled, and the figures above — the
  incumbent's 2 of 478 and the promoted candidate's 48 — are what the labelling
  produced.
  **Phase 3's spike ran on 2026-08-12 and answered all three of its questions,
  plus one it was not asked.** `tract` runs comic-text-detector unchanged —
  seventeen operators, all supported, opset 11, no `NonMaxSuppression` in the
  file at all because upstream suppresses in Python — so the `ort` fallback was
  never reached and the offline stance holds, at 3.8–4.0 s per page on CPU, flat
  across page sizes. The weights are redistributable and carry an **attribution
  obligation** the design did not know about: the training data is Manga109-s,
  the commercially-approved subset, not the academic-only Manga109 whose clause a
  reader meets first. And the question the spike was scoped to add: over the
  20 pages of the four Japanese `raw-mono` editions, where the promoted classical
  candidate scores **0 of 167** dialogue boxes, the model's block head proposes
  **147 blocks against 167 dialogue boxes**, sitting on the balloons. That is a
  count of proposals and a look at four overlays — **not a score**.
  **Phase 3 produced the score the same day, and the model clears the rule on
  both terms.** `model_experiment.rs` ran both candidates over the same 45
  pages, the same nine editions, the same 478 scored boxes, through the same
  `score_page` at the same provisional `COVERAGE = 0.5`:

  | outcome | `detect_page` (L2b) | `model/blk` |
  | --- | ---: | ---: |
  | 1:1 | 48 | **130** |
  | merged | 73 | 97 |
  | split | 152 | 6 |
  | fragment | 81 | **137** |
  | missed | 124 | 108 |
  | spurious | 303 (6.73/page) | 3 (**0.07**/page) |

  **The model row above is the second measurement of it, and the first was
  wrong.** It read `118 87 32 133 108 3` until `14cb71c`, which fixed a defect
  in this project's port: `suppress` was class-aware, faithfully to YOLOv5, so a
  block whose grid cells disagreed about which of the two class channels won
  survived **twice**, as two boxes at an IoU of 0.94 to 0.99. Upstream gets away
  with that because `inference.py` follows NMS with `group_output(...)`, which
  regroups the survivors; this port has no such stage, so the duplicates reached
  the scorer. The fix is `agnostic=True` on an existing YOLOv5 flag — the other
  setting of something already there, not a new mechanism and not a new
  constant. The classical row did not move. **The score could not see this**; the
  cross-edition grouping measurement below found it, and the pixels confirmed it
  (`gotoubun-v01` raw 165 against `en-mono` 157: one balloon, two boxes), at
  **40 of 161 English regions carrying such a twin against 0 of 147 Japanese
  ones**. **The reasoning was written down before the re-run**, deliberately,
  because the numbers move in the direction that flatters the model. **`missed`
  and `spurious` did not move** — 108 and 3 in both measurements — and that is
  the check, since removing duplicate boxes cannot change what was never found.
  **Every one of the twelve extra dialogue hits is English**, which is the
  second check: the Japanese pool is 47 of 167 before and after, per edition as
  well as pooled, while the English pool moves 65 of 222 to 77 of 222 — exactly
  where the twins were, so the arithmetic corroborates the diagnosis rather than
  merely failing to contradict it.
  **The verdict, stated against the rule fixed on 2026-08-11 rather than
  against the outcome:** term 1 is 130 against 48, term 2 is 0.07 against 6.73,
  and both sit inside the ceiling of 10.7 truth boxes per page. **Both pass.**
  The **0 of 167** dialogue boxes this entry has quoted since L2b is **47 of
  167** for the model — untouched by the fix above — and it is not bought by
  trading the English side away: the English pool moves 43 of 222 to 77 of 222
  at the same time, and the model takes more 1:1 than the classical detector in
  every one of the nine editions.
  **Three things are recorded beside that, because a summary without them would
  be dishonest.** `fragment` rose from 81 to 137: the model's failure shape is
  different, not absent. Sfx recall moved the **wrong way**, 5 of 63 to 2 of 63
  — the classical detector finds more sound effects than the model does. And
  the model's 0.07 spurious per page is numerically the **incumbent's** figure
  from the "8.6% of the page" paragraph above, reached from the opposite cause:
  the incumbent's 0.07 was that it barely proposed anything at all, while 137
  fragments prove this one proposes plenty. The rule's second term cannot tell
  those apart; the first term is what does.
  **Promoting the model into the binary remains a separate decision** (design
  §9), and it now carries two things with it: the **Manga109-s attribution
  obligation** — anything shipped with these weights must state clearly that
  Manga109-s was used — and a `COVERAGE` this result is more sensitive to than
  any before it, since `blk` hugs the glyphs where a hand-drawn box carries
  margin, so the model's predictions are systematically *tighter* than the
  truth. That interaction is predictable in direction and unmeasured in size.
  **That decision was taken on 2026-08-12, and the answer is not yet — for a
  reason that is not about the measurement.** The model stays a `#[cfg(test)]`
  experiment; `detect.rs` keeps shipping; nothing gains a 95 MB dependency. The
  reasoning, recorded because a deferral without one decays into a to-do:
  **a text detector may not be this project's to own.** Panels, the iOS comic
  reader, has a text-selection mode that appears to lean on a native iOS
  capability rather than a bundled model — Apple's on-device text recognition —
  and if the platforms that matter already carry text detection, then shipping
  95 MB of weights, a fetch step, a licence obligation and a 4 s/page inference
  is a large cost to take on for something the operating system may hand over
  for nothing. Nobody has looked. Until somebody has, committing to a
  text-finding model would be choosing a mechanism before surveying the
  alternatives, which is the failure this whole sub-project has been correcting
  for.
  **This does not block anything, and that is the point.** L2b's pairing works
  against `Region`s and does not care what produced them; the model is
  measurable today, gated, and reproducible in one command. What a survey
  would have to answer: what the browser PWA can use, since a native iOS
  capability does not reach it and the reader is a PWA first; whether a native
  shell is close enough to change that; and what such an API returns — lines,
  words, or the *blocks* pairing needs. A native text API that returns word
  boxes solves a different problem from the one measured here.
  **That survey ran on 2026-08-12 and all three questions are answered, in the
  direction that removes the alternative rather than supplying it.** `notes.md`,
  "What the platforms already do, and the two halves no public API puts
  together"; spec §4.3 carries the dated settlement. **The browser: nothing.**
  Shape Detection's `TextDetector` is flag-only in Chrome, absent from Safari
  and Firefox, on no standards track — and its specification scopes it to
  **Latin-1 text**, so Japanese is not badly supported, it is out of scope by
  definition. WebNN is origin-trial only, estimated at 2027. Safari's Live Text
  on an `<img>` is user-invoked and hands the page no coordinate.
  **The headline is on the native side: Apple has two text APIs and each has
  exactly one of the two properties L2 needs.** `Vision`'s
  `RecognizeTextRequest` returns bounding boxes and **cannot read vertical
  Japanese** — Apple's own DTS answer is to rotate the page upright yourself
  first, which presumes the detection this sub-project exists to do.
  `VisionKit`'s `ImageAnalyzer`, the Live Text framework, **does** read vertical
  CJK and **exposes no positions at all**; the geometry is private to
  `ImageAnalysisOverlayView`. The API that can read the writing system returns
  no coordinates; the API that returns coordinates cannot read the writing
  system. **So the Panels comparison does not transfer**: Panels needs
  *selection*, which Apple's overlay draws without ever handing the app a
  rectangle, and taiyaku needs a rectangle on both editions or there is nothing
  to pair. Android's ML Kit v2 does return `TextBlock`→`Line`→`Element` with
  boxes and supports the Japanese script — the right shape, on a platform this
  reader does not target, silent about writing direction and unmeasured on
  manga.
  **And the question's own premise was the wrong shape**, which is the part
  worth keeping: "what the browser PWA can use" assumed detection has to happen
  where the reading happens. It does not — the PWA is served by `taiyaku serve`,
  which already decodes, caches and serves per-work JSON, so a `regions` route
  beside `/truth` is the same shape as what is there and 4 s/page is a cached
  server cost. The OS-API question binds only in an architecture that is neither
  built nor planned: a static PWA against Stump, or a native app.
  **What it changes about the decision is less than it looks.** The deferral's
  first half — the platforms may do this for nothing — does not survive. Its
  second half is untouched: 95 MB, the fetch, the Manga109-s obligation, the
  dependency and 4 s/page cost exactly what they did. There is simply no cheaper
  alternative left to wait for, and that is **not** an argument to promote,
  because the model still finds only 47 of 167 Japanese dialogue boxes and the
  confidence sweep showed the other 120 are under no threshold. **No route is
  free.** The cheapest unexplored one is unchanged: the `seg` head, out of the
  same forward pass at no extra cost, firing on lettering `blk` proposes nothing
  for. **This survey is weaker evidence than anything else cited here and says
  so** — no API was run, on any device, against any page of this corpus; it
  settles what the vendors document and nothing about how it behaves on a manga
  raw.
  The 30 sealed held-out pages were **not** opened. `notes.md`,
  "comic-text-detector through `tract`: four questions, and the head that
  answers them", "The model candidate, scored against the same 45 pages" and
  "The model against itself across the two editions: does it group the same way";
  spec §4.1 and §4.2 carry the dated settlements.
  **The confidence floor was swept on 2026-08-12 and `BLK_CONF` stays at 0.4**,
  which is a refusal rather than an oversight. The premise was sound — the rule
  caps spurious at 10.7 per page and the model sits at 0.07, so there was ~150x
  of unused precision to trade for recall — and the trade is real: at a floor of
  0.02, `missed` falls 108 to 57 and 1:1 rises 130 to 147, still 19x under the
  ceiling. **But Japanese dialogue is flat at 47 of 167 across seven of the
  eight rows.** Every extra hit is English. The 120 Japanese boxes the model
  misses are not low-confidence detections under the floor; `blk` does not
  propose them at any floor, so no threshold reaches them. The gain also worsens
  the failure *shape* — `split` 6 to 36, `fragment` 137 to 162 — so it would
  trade 17 English matches for 55 more wrongly-shaped boxes and zero Japanese
  ones. **The promotion rule cannot arbitrate this**: its second term is slack
  across the whole table, so by term 1 alone the rule would adopt 0.02, and
  applying it here is using a rule written to compare a candidate against an
  incumbent to pick a parameter inside one candidate. `notes.md`, "Where the
  confidence floor goes, and the thing it turns out not to buy". The sweep is a
  dev-set overfit and says so; the held-out 30 stay sealed.
  **The `merged` 97 above was then asked the only question that decides whether
  it costs anything, on 2026-08-12.** The model draws one box over two adjacent
  balloons where the labeller drew two. That is free if it merges the *same* two
  on the raw and on the English page — L2 pairs region to region, so both sides
  agree and a reader still lands on the right balloon — and it breaks pairing if
  it merges on one side only, which no relabelling could fix because it is model
  behaviour rather than a labelling convention. A second gated test in
  `model_experiment.rs` runs the model over the five labelled `raw-mono` pages of
  each of the four works and over each page's English counterpart, resolved
  through `Sidecar::runs_of` and `counterpart` in **slots** rather than file
  pages, and compares by **centroid containment** — not IoU, which would be
  measuring the writing system, since a vertical Japanese balloon and its
  horizontal English counterpart share a centre and share almost no area.
  **19 of the 20 pairs compared**, one skipped because its counterpart slot fell
  on a joined spread and splitting that page is L2b's problem, not this one's;
  147 raw regions against 141 English. Of 147 raw centroids, **124 land in
  exactly one English region**, 23 in none and 0 in two or more; of 141 English
  centroids, 115 land in exactly one raw region, 25 in none and 1 in two or more.
  **The headline is 5 of 147 raw regions holding two or more English centroids,
  all 5 of them real grouping disagreements, and 0 of 141 the other way.**
  **This measurement found a defect before it answered its own question.** Its
  headline first read 23 of 147 of which only 5 were real, and the other 18 were
  the class-aware suppression fixed at `14cb71c` and recorded above — one balloon
  carrying two English boxes. After the fix the two printed figures, raw
  containment and containment after discounting near-identical twins, are **5 and
  5: they converged**. That convergence is the check, and a stronger one than the
  number merely falling: the two routes measure the same thing and disagreed only
  because of the artefact, so their agreeing says it is *gone* where a smaller
  number would only have said it was *smaller*. Both figures still print, and so
  does the twin line — now 0 of 147 and 0 of 141 — so a regression shows up as
  them parting again.
  **What it says: the labelling-convention mismatch costs almost nothing.** About
  3% of raw regions are genuine grouping disagreements and the mirror case is
  zero, so when the model draws one box over two touching balloons it
  overwhelmingly does the same on both editions and pairing maps region to region
  cleanly. That **retires relabelling as something this project needs to do for
  that reason** — the labelling convention itself is written down at `f165521`,
  in `crates/taiyaku-core/src/truth.rs`, and the open question it left was
  whether L2 pairing needs a unit consistent *across editions* rather than one
  per balloon. It does not, here. `kobayashi-v01` raw 36 against `en-mono` 38 is
  one of the five real ones: two adjacent balloons boxed separately in English
  and together on the raw. The test asserts none of this — whether 5 in 147 is
  tolerable is a person's call, and a check that fails when the model improves is
  not a measurement. **What this does not settle:** page fractions correspond only
  approximately across editions of different aspect (0.669 against 0.703 on one
  real pair, deltas of ±0.02 to ±0.04 printed per pair), which the test
  deliberately does not correct for — fitting that transform is L2b's job — and
  one page pair is unmeasured because its counterpart landed on a joined spread.
- **Art book linking**, if it is worth it — needs a different hash, and roughly one
  art book page in three has any manga counterpart at all.

**L1 geometry left this list on 2026-08-08, and the line it left behind is kept
rather than deleted.** It read:

> **L1 geometry** — affine transform per pair, so a swap preserves zoom and pan.

Wrong on both counts, which is exactly why it is worth keeping: a list of future
work is also a record of what the project expected of that work. There was no
zoom for a swap to preserve — building it was most of the milestone, and the
design records that inversion as the largest thing it settled — and no per-pair
transform is stored, in the sidecar or beside it. §5 above is what shipped.

## Corpus

Eleven series with both a raw and at least one English edition. Held on a NAS, never
committed, located at build time. Beyond the four the design started with, the
material now covers repagination at 2:1 and 4:1, editions published entirely as
joined spreads, letterboxed scans, archival TIFFs at 430 MB per page, three
independent scans of one volume, and at least two files that are not what their
names claim.

It now also holds **negative** material, which it lacked: `incoming/needles/`
keeps four short unrelated pairings that the solver accepts and should not. A
corpus of things that ought to work can only measure one half of a guard rail.

The awkward material is the valuable material. Every guard rail in the solver exists
because something in this corpus broke without it.

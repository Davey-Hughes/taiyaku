# Two tiers for credit pages, and a corpus that can judge them — design

**Status: settled in brainstorming on 2026-08-13. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

## Where it came from

The question on the table was how good credit-page detection actually is. It
has a number — **precision four of five, recall four of five** — recorded in
`notes.md` under "A second group's habits, and what the bound is worth", and
reproducible with `scripts/credit-survey.sh corpus`. The number was judged not
good enough for what the feature is wanted for, and the target was named:
**`skip` on by default**, where a false positive costs a reader part of the
book rather than costing a reviewer a glance.

That target is what makes this a design rather than a tuning exercise. Today
`skip` defaults to **off** (`web/src/modules/credits.ts:68`) and the list only
reaches a sidecar through a person running `credits --write`, so the heuristic
has two safety nets and 4/5 is a statement about reviewer effort. Remove the
first net and it becomes a statement about reader risk.

## 0. Two phases, and the second is gated on the first

This design is deliberately too large for one implementation plan, and the
split is load-bearing rather than administrative.

**Phase A — material, survey, ground truth.** §2's three waves, §7's fixtures,
§8's two denominators. It ends with a number: precision and recall over 15
editions instead of 2, and a test that fails if either moves.

**Phase B — the tier rule.** §4, §5 and §6: the corroborators, the second
sidecar list, the reader default. Every threshold in Phase B is chosen from
Phase A's output.

**B does not start until A has produced its numbers.** The whole argument of §1
is that a rule tuned against five positives is fitting five pages; building the
tiers first and measuring afterwards would reproduce exactly the mistake this
design exists to correct. Phase A is worth shipping alone — it makes the
existing 4/5 trustworthy or shows it was optimistic — and if it shows
shape-plus-colour failing outright, Phase B gets redesigned rather than
implemented.

## 1. Why the rule cannot be tuned out of its one error

`notes.md`'s candidate table holds one false positive: Kaguya page 212, `c10`
local 19 of 20, a full-colour magazine announcement — the publisher's own
matter, not the scanlation group's.

**It is structurally identical to a true positive.** It is unpaired for exactly
the reason a real credits page is unpaired: `raw-mono` is a tankōbon scan and
that page belongs to the magazine serialisation, so nothing on the raw side
matches it. It is an isolated run of 1 at its member's final index. Signal 1
reads position in the alignment and on position these are the same page. No
threshold moves this one, and that is the finding that makes a content signal
mandatory rather than optional.

**The obvious corroborator loses, measured.** Signal 2 — dhash near-duplicate
of a page in a sibling member — is three of five. It misses both staff-credit
pages, at Hamming distances 93 and 99, because each appears exactly once in the
edition, so gating signal 1 on it would veto the two true positives signal 1
uniquely finds. Its own tightest match in the whole Go-Toubun edition is the
volume **cover**, appearing in both `c000` and `c005.5` at distance 2 — a page
the reader wants. It is a duplicate detector, and duplicated front matter is as
common as duplicated credits.

**The bound cannot move either.** Every credit page found anywhere in the
corpus is a run of exactly 1, five for five. The only run of 2–3 that exists is
Kaguya pages 2–4, two inside-cover omake and the digital-distribution notice.
Front matter.

**And the real blocker is the denominator.** Both metrics are over five known
credit pages. One page moves either by twenty points. Any rule change evaluated
against five positives is fitting five pages, and an improvement could not be
distinguished from a coincidence. That is why the material comes first here and
the rule second.

## 2. The material, surveyed 2026-08-13

`/mnt/daveynet/nfs/media 1` holds **14 directories named "Fan Translated",
1,924 `.cbz` between them**. Counted on this machine; the table below records
what each can contribute and why several contribute nothing.

| Series | Fan packaging | Starts at | Counterpart | Use |
| ------ | ------------- | --------- | ----------- | --- |
| Kobayashi | chapters, `v1`–`v16` markers | v1 c1 | raw, in corpus | wave 1 ×3 |
| Kaguya-sama wa | chapters, `v1`–`v28` markers | v1 c1 | raw, in corpus | wave 1 ×2 |
| \[Oshi no Ko\] (`Manga/`) | **volumes** `v01`–`v13` | v01 | raw, in corpus | wave 1 ×3 |
| Dungeon Meshi | chapters | c001 | official EN | wave 2 |
| Sousou no Frieren | chapters | c001 | official EN | wave 2 |
| Domestic na Kanojo | chapters | c001 | official EN | wave 2 |
| Sono Bisque Doll | chapters | c001 | **none** | unalignable |
| Ore dake Level Up na Ken | chapters | c001 | official EN | wave 3 |
| Go-Toubun | chapters, no markers | c000 | raw, in corpus | excluded — §9 |
| Bocchi the Rock! | chapters, partial markers | c82 | official EN | excluded — §9 |
| Maoujou de Oyasumi | chapters | c378 | official EN | excluded — §9 |
| \[Oshi no Ko\] (`Manga (Chapters)/`) | chapters | c131 | raw, in corpus | excluded — §9 |
| Mahou Shoujo ni Akogarete | **volumes** `v01`–`v12` | v01 | official EN | excluded — §9 |
| Kaguya-sama wo Kataritai | chapters, `v01` markers | v01 c001 | **none** | unalignable |

**Scope is 14 fan editions**, against the two the current number rests on.

**Two of the fourteen candidates were dropped for want of a counterpart, and
one of them was dropped late.** Kaguya-sama wo Kataritai has neither a raw nor
an official edition on the server. Sono Bisque Doll was carried in wave 2 of an
earlier draft of this design on the strength of a cross-reference that counted
`Manga/Bisque Doll de Chu ♡` as its official edition; that is a different work,
and checking by name rather than by count found it. There is no official
English *Sono Bisque Doll wa Koi wo Suru* and no raw, so it cannot be aligned
and it contributes nothing. Recorded rather than quietly deleted, because a
counterpart that a listing appears to offer and does not actually hold is a
mistake the next survey can repeat.

### The three waves

**Wave 1 — eight editions, no convention change.** `kobayashi-v01/v03/v08`,
`kaguya-v03/v08` and `oshinoko-v01/v03/v08` each gain an `en-fan`, aligned
against the raw already sitting in that corpus directory. Kobayashi's group,
`Cirno9baka`, is new. Kaguya v03/v08 test whether Psylocke's measured habit of
printing no credits at all holds across volumes of one series, which is a thing
the current sample asserts from one volume.

**Wave 2 — three editions, three new groups, one convention change.** Dungeon
Meshi, Frieren and Domestic na Kanojo v01, each fan against
*official English* rather than a raw. The corpus has never used an official
edition as an alignment hub. `credits` itself is indifferent — it passes no
`--against` and lets the sidecar's hub decide — but `corpus/README.md`'s layout
and `scripts/credit-survey.sh`'s own header both state that `raw-mono` is every
work's hub. Whether the script needs any change at all is **unconfirmed**; see
§10.

**Wave 3 — one edition, a deliberate stress case.** Ore dake Level Up na Ken is
a full-colour webtoon. If §4's colour corroborator is real, this is where it
breaks. Better that it break on purpose in a survey than by surprise on a
reader.

### What the Oshi no Ko volumes are for, and the correction that got them in

This design initially excluded `Manga/[Oshi no Ko] - Fan Translated` on the
grounds that it was not a scanlation: its pages carry
`[web] [MANGA Plus by Shueisha Inc.] [Oak]`. That reading was wrong. The tag
names the **source of the raw** — the Japanese web release — not the
translator; `Oak` is the group, and `[raw-d]`/`[edited]` are its own marks. It
is a fan translation, and the edition carries **no credit pages at all, only
small credit notes set onto existing pages**.

That makes it the most useful negative material available, and it lands three
tests in one file:

1. **A precision test at the rule's highest-risk position.** Each volume is a
   single member, so signal 1 can fire at most once — on a run at the volume's
   final index. Page counts measured on this machine: raw-mono holds 223, 197
   and 201 files for v01/v03/v08 against the fan volumes' 219, 195 and 195
   pages. The fan volume plausibly ends on a page the raw lacks. One candidate
   per edition, and every one of them a false positive if it appears.
2. **The volume-packaging case** of §5, on material with a raw already present.
3. **Ground truth for a credits-as-a-note edition**, which is the shape §9
   records as beyond any page-level rule.

## 3. Signals, and what each is actually worth

**Signal 1 — shape.** Unpaired against the hub, isolated, at a member's final
index. Four of four on Go-Toubun, one false positive on Kaguya. Keeps its
place as the candidate generator.

**Colour in an otherwise-monochrome edition — the primary corroborator, and
new.** This is the design's one real bet. Kaguya's table rejects page 7 as a
"colour chapter opener", page 105 as a "colour chapter title page", and page
212 — the false positive — as "the publisher's **colour** announcement". In a
monochrome scanlation, colour marks publisher and magazine matter, while a
scanlation group's own card is near-universally greyscale. The three Go-Toubun
donation cards are near-black; the `c005` staff page is a greyscale meme panel.

This supersedes the luminance-and-variance idea the conversation opened with.
Luminance catches the near-black donation cards and would miss the `c005` meme
panel; colour catches the false positive we actually have. **It is a hypothesis
and it is unmeasured** — §10.

**dhash recurrence — the secondary corroborator.** Three of five alone, and
worthless as a gate for the reason §1 gives, but strong evidence when it does
fire. It is what says the three donation cards are one card printed three
times.

**`ComicInfo.xml` — recorded, and deliberately not wired into the rule.**
Kobayashi's v1 c1 carries `<ScanInformation>Cirno9baka</ScanInformation>`, plus
`<Volume>`, `<Number>`, `<Title>` and a MangaDex chapter URL, read on this
machine. Kaguya's twelve chapters carry `ScanInformation: Psylocke Scans`.
Dungeon Meshi, Frieren and Bisque Doll carry none. `status.md` records 12 of 57
archives holding one.

It names a group and never a page, so there is no sound way for it to promote
or demote a *candidate*. In this design it is **survey output only**: the
`credits:` line and the per-run table report which group the archive claims,
which is what lets a reviewer say "this is Cirno9baka's habit" when adjudicating
candidates, and what would let a later design act on it once several editions
per group exist. It does not enter §4's tier rule. Where present it also hands
over the chapter-to-volume mapping for free, which is what makes wave 1's
ingest mechanical.

**Filename role tags — rejected as a signal.** Oak tags `[Cover]` and `[ToC]`
in every entry name. Both corpus fan editions use bare numerics — `048.jpg` on
Go-Toubun, `02.png` on Kaguya — checked on this machine. It is one group's
convention, not a property of fan editions, and nothing should be built on it.
Whether Oak tags anything credit-like is worth one look during wave 1 ingest
and nothing more.

## 4. The tier rule

The sidecar's flat `credits` list becomes two.

**Skip tier** — signal 1 **and** at least one corroborator. Skipped by the
reader.

**Review tier** — signal 1 alone. Listed in the panel, marked as unconfirmed,
and left in the reading flow.

On today's corpus that puts the three recurring donation cards in the skip tier
and Kaguya 212 in the review tier. The `c005` staff page recurs with nothing
and is greyscale, so it lands in **review** — skip-tier recall of 3 in 5 buying
skip-tier precision of 3 in 3. That trade is the right way round for
skip-on-by-default: a missed credits page costs a reader one page-turn, a
swept-up story page costs them part of the book.

**When no corroborator is available**, the candidate lands in review. The rule
degrades toward showing the page.

**Rejected: one list with a high bar.** Simplest, no format change, but it
throws the review tier's information away — the panel is the feature that makes
a heuristic's second-guesses useful, and it already exists.

**Rejected: a per-candidate confidence score with the cutoff in the reader's
settings.** Most flexible and most machinery, and it pushes onto readers a
judgement they have no way to evaluate.

**Rejected: a fitted content descriptor** — luminance, stddev, saturation, ink
coverage, text density, with the tier boundary learned. It is the most general
option and the only one that would reach credits at a member's *first* index,
which nobody has measured. With five positives it is indefensible. It stays on
the list for after the widened survey, if shape-plus-colour is shown to fail.

## 5. Volume-packaged editions get a verdict, not silence

When a fan edition ships one archive per volume, signal 1 has exactly one
position it can fire on and signal 2 has no sibling member to compare against.
The rule is structurally blind, and today it would report no credits in a way
indistinguishable from having found none.

`credits` states that such an edition is **not surveyable by shape**, and its
skip tier is **empty** rather than holding one uncorroborated candidate.

This follows the no-silent-caps discipline the repository already applies to
surveys, and with `skip` on by default it is the difference between a reader
losing a page and a reader keeping one. Two of the fourteen fan editions on the
server are packaged this way, so it is not a hypothetical branch.

## 6. The sidecar, and the reader

`sidecar.md` is normative, so the second list is a format change with a
migration note. An edition's existing `credits` list is read as the skip tier,
which is the reading that preserves today's behaviour for a sidecar written
before this change.

`taiyaku credits` prints both tiers in its table and writes both under
`--write`. Its one fixed-field `credits:` line gains a field for the review
count; `work=` stays last, for the reason `notes.md` gives at length.

In the reader, `creditsModule`'s default becomes `skip: true`. `CreditsPanel`
lists both tiers, the review tier marked as unconfirmed. Skipping still never
renumbers a slot.

## 7. Ground truth becomes a file

Today the 4/5 lives only in `notes.md` prose. `credits.rs`'s tests pin flag
handling, table shape, `is_candidate`'s bound and ordering — **none of them
pins the corpus outcome**, so a regression in the rule would move the survey's
candidates and ship green.

Each surveyed edition gets a checked-in labelled fixture holding its unpaired-run
structure and the adjudicated verdict on every candidate. `corpus/` is
gitignored, so the fixture carries run shapes and page indices and never
images, which is what lets it run in CI. `credit_pages()` already takes runs
rather than pages, so the seam exists.

That is what makes "we improved it" a claim a test can refute.

## 8. Testing, and the recall denominator

**Precision** is measured over every candidate the rule emits across all 15
editions, each extracted and looked at. That is the discipline `notes.md`
records as the only thing that has ever found an error in this feature.

**Recall needs a denominator of known credit pages, and an exhaustive sweep
does not scale to 15 editions.** The 226-page luminance-and-variance ranking
done to earn Kaguya's negative was affordable once. So:

- **Exhaustively swept:** the two existing editions plus wave 1's eight. Recall
  is reported over this subset only.
- **Candidates only:** waves 2 and 3, contributing to precision alone.

Two numbers with stated denominators, rather than one number with a soft one.
Both denominators are named wherever either figure is quoted.

## 9. What is deliberately not done

**L2 text regions.** A credits page is text-dense with no balloons, and that is
probably the strongest signal available — but it is gated on promoting the
detector into the binary, a decision deferred on 2026-08-12 for want of a
reason rather than for want of effort. Out of scope, and named so it is not
mistaken for an oversight.

**Go-Toubun v03/v08 and Bocchi.** Both need a chapter-to-volume mapping that is
not in their filenames. Wave 1 and 2 already take the sample from 2 editions to
15; this is deferred, not refused.

**Maoujou de Oyasumi and the chapter-level Oshi no Ko.** Fan coverage starts at
c378 and c131, with no counterpart volume in the corpus.

**Mahou Shoujo ni Akogarete.** Volume-packaged with no raw. It would exercise
§5 and nothing else, and the Oshi no Ko volumes already do that against raws
that exist.

**Credits typeset onto a story page**, and **a credits page the solver paired**
(Go-Toubun `c000` local 43, confirmed by extraction). Both are recall ceilings
no page-level rule reaches, and neither is addressed here.

## 10. What could not be measured, and what is a guess

**The colour corroborator is unmeasured.** §3 argues it from three rows of
Kaguya's candidate table. It has not been computed over any edition, and no
figure for it should be quoted until wave 1 is surveyed. If it fails, §4's skip
tier falls back to dhash recurrence alone and the design is materially weaker.

**Whether `credit-survey.sh` works unchanged against an official-English hub.**
The script passes no `--against`, so it should follow whatever hub the sidecar
names, but this has not been run.

**Whether the Oshi no Ko fan volumes align cleanly against the corpus raws.**
Page counts are close — 219/195/195 against 223/197/201 — which is the shape of
a good pairing, but no alignment has been run and a `FRAGMENTED` verdict would
weaken those three editions the way it weakened Kaguya's.

**How many known credit pages the widened corpus actually holds.** The
expectation of 15–20 is arithmetic from four groups, not a measurement, and
Kaguya's contribution of zero is the reminder that an edition can add pages and
no positives.

**Whether `skip: true` is safe at the end of this.** That is the decision the
survey exists to inform, and this design does not presume it. If precision on
the skip tier does not clear a bar set *before* the number is known, the
default stays off and the two tiers are still worth having.

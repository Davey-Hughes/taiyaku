# Aligning two editions of a book, measured before the reader is designed — design

**Status: settled in brainstorming on 2026-09-30. Not yet built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat, a figure asserted here was measured on this machine
rather than recalled, and a guess says it is one.

This is **phase 1 of the books work** described under "Later" in
`docs/status.md` ("Books: two editions side by side, and an apparatus set
properly"). The user decided on 2026-09-30 that **alignment is measured before
the reader is designed**, so that the reader is built around what this
measurement says a block and an anchor must be. Nothing here builds a reader,
a sidecar format for books, or hand-correction; §10 lists what waits.

## Where it came from

A read-only survey of the library's EPUBs (`docs/notes.md`, "Books: what the
library's EPUBs hold") found the four facts this design is built on:

- **Two translations in one language keep the original's paragraphing.**
  Oliver Ready's and Pevear and Volokhonsky's Crime and Punishment differ by
  at most three paragraphs a chapter over the eighteen chapters before Part
  Three V, with character counts within 0.97–1.02 of each other.
- **A translation into another language does not.** The Korean Harry Potter 1
  has 1.03 to 1.32 times the English's paragraphs a chapter, while its length
  stays between 0.62 and 0.75 of the English.
- **Structure lies.** Ready's contents leave out Part Three VI and its text
  never marks that chapter's heading as a heading, so an aligner that followed
  the table of contents would be one chapter off from there to the end.
- **Some books carry their own alignment.** Heaney's Beowulf numbers the Old
  English and the translation every ten lines; Hollander's and Mandelbaum's
  Inferno link their Italian to their English tercet by tercet.

The first three are the manga problem one level down — the README's
"'show me page 47 of the other edition' is not a lookup" — with paragraphs
for pages. The fourth is free ground truth, which L2 had to label by hand.

## 1. What the measurement answers

Four questions, each feeding a decision the reader phase has to make:

1. **Can one reading-order dynamic program with n:m groups align every kind
   of pair in §2**, or does each kind need its own method? The user
   allowed same-language and cross-language pairs to be aligned differently.
2. **Which match score is enough, per kind** — length alone, length with
   cheap anchors, or multilingual embeddings — and what does the winner cost
   a book?
3. **What unit can a reader keep in step?** The user's answer was "generally
   paragraph, looser where a pair needs it"; this measures how loose.
4. **Is a scanned PDF's OCR text good enough to align**, and how much cleanup
   stands between it and paragraphs?

**No pass bar is set here.** Every acceptance rule stated for L2's text-block
detector before its ground truth existed failed *as a rule* (`docs/status.md`,
"In-volume chapter detection", under "Later"). Each question's test prints tables and
asserts nothing about them; the decisions are made reading those tables, and
recorded in `docs/notes.md` with them.

## 2. Material

**Local copies, not the NAS.** Every book is copied once into a gitignored
`corpus/books/`, one directory per pair, and read from there. The NFS mount
was answering one share with "Stale file handle" on the day of the survey,
and a measurement that depends on a mount can fail for reasons it cannot
report. The originals are never opened for writing. Copies, not hardlinks: the
NAS is another filesystem.

| Kind | Pair | Answer key |
| --- | --- | --- |
| Same-language prose | Crime and Punishment — Ready (Penguin) ↔ Pevear and Volokhonsky (Vintage) | hand key, Part Three VI |
| Same-language prose | Anna Karenina — Bartlett (Oxford) ↔ Schwartz (Yale) | none: generalisation only |
| Cross-language prose | Harry Potter 1 — English ↔ Korean (Pottermore) | hand key, chapter 1 |
| Cross-language verse | Beowulf — Old English ↔ Heaney's translation (one EPUB) | free: line numbers |
| Cross-language verse | Inferno — Italian ↔ Hollander's English (one EPUB) | free: tercet links |
| Same-language verse | Inferno — Hollander ↔ Mandelbaum | free: through the shared Italian |
| PDF ↔ EPUB, same text | The Great Gatsby — Cambridge (scanned PDF with an OCR layer) ↔ Scribner | hand key, chapter 1 |

**Why these key chapters.** Each is the hardest available case of its kind,
not a typical one. Part Three VI is the chapter Ready's contents omit, so the
whole-book alignment must find it without being told it exists. Harry Potter's
chapter 1 has the highest Korean-to-English paragraph ratio of the seventeen
(1.32). Gatsby's first chapter is chosen for being first, and the OCR noise it
carries is what the pair measures.

**Beowulf and the Italian-and-English Inferno are one file each**, two texts
in one EPUB. The extractor reads them as two editions by spine range — the
Old English is `part0008`–`part0010` and the translation `part0011`–`part0013`
— and the key extractor states those ranges rather than inferring them.

**Hollander and Mandelbaum are keyed through the Italian**, as manga editions
are routed through a hub: English tercet *k* in one maps to Italian tercet
*k*, which maps to English tercet *k* in the other. Both print the Italian
beside their English; whether the two Italians and their tercet numbering
agree is checked (§3) rather than assumed.

## 3. Answer keys

**Format.** One JSON document per keyed pair, beside the copies in
`corpus/books/<pair>/key.json`, in the spirit of manga's `truth.json`: a list
of links, each a group of block ids on side A and a group on side B, where an
empty group on one side marks text the other side does not have. Block ids are
the extractor's (§4). The key holds ids, not text, so it says nothing the books
do not.

**Hand keys are drafted by reading, never by running a candidate.** Claude
aligns the three key chapters by reading both texts side by side. A key drawn
from any candidate's output would anchor to that candidate's mistakes and could
not fairly score it. The user then **spot-checks twenty links per key, drawn at
random** by a seeded draw the experiment prints, reading each link's two sides
and marking it right or wrong. A key with a wrong link among its twenty is
corrected and redrawn before any figure is read against it.

**Free keys are counted before they are used.**
- *Beowulf*: 318 numbered lines per side, 10 to 3180. The translation's number
  cell changes class from `sgc3` to `num` at line 1600 (`part0012.html`), so an
  extractor keyed on one class returns 159; the count is asserted at 318.
- *Inferno, Italian to English*: every tercet link must resolve to an element
  that exists, in both directions, and the count of Italian tercets must equal
  the count of English ones per canto.
- *Hollander to Mandelbaum*: every Italian tercet id in one must have a
  counterpart in the other, canto by canto; a disagreement is printed and
  that canto left out of the key rather than guessed.

**What a verse key measures, and what it does not.** A line-numbered or
tercet-linked key only has links every ten lines or every tercet, so it scores
landing at those anchors. It cannot say whether the lines between them are
matched well, and the tables say so beside every verse figure.

## 4. Extraction: `taiyaku-read::book`

The one component the reader phase reuses, so it is built to stay: an EPUB
becomes an ordered list of `Block`s, one per block-level element that carries
text of its own.

```rust
pub struct Block {
    pub id: BlockId,         // spine href + ordinal within that document
    pub kind: BlockKind,     // Paragraph | VerseLine | Heading | NoteBody | Other
    pub text: String,        // whitespace collapsed, soft hyphens removed
    pub line_number: Option<u32>,
    pub page_marker: Option<String>,
    pub links_out: Vec<Href>,
}
```

**The id is deliberately the simplest thing that is stable within one file.**
Whether it survives a re-scanned or re-published file is the reader phase's
question — the research behind this design found text-quote selectors to be
the robust anchor, and CFIs and progressions not — and phase 1 does not need
to answer it.

**Strict XML, not an HTML parser.** 763 spine documents across fifteen books
parsed as strict XML with no failure, calibre's conversions included
(`docs/notes.md`). So the parser is `roxmltree`, and a document that fails to
parse is **refused and reported by name** rather than recovered, since a
recovering parser guesses and the guess is invisible. *An earlier draft of
this design chose html5ever on the assumption that calibre's output is not
reliably XML; the measurement said otherwise, and the assumption is recorded
here because it was stated as fact while it was a guess.* Spine paths are
resolved against the package document's directory: Standard Ebooks' Captain
Blood names a spine document with `../` in it.

**Note bodies leave the reading flow.** Hamlet's commentary sits in files of
its own after the play; aligned as text, it would be matched against the other
edition's play. A block is a `NoteBody` when it is the target of a reference
and either links back to that reference, or the reference is a superscript or
a number of at most three digits or one or two letters — the rules KOReader
and foliate-js apply, since none of the fifteen books marks a note
semantically. **Line-keyed commentary is not detected in phase 1**: Longman's
Paradise Lost notes link to nothing, so they would stay in the flow; Paradise Lost
is not in the pair set. The experiment reports how many
note bodies each book yields, beside the count of its publisher's note class,
so a detector that finds none on a book full of notes is visible.

**PDF (the OCR layer).** `pdftotext -bbox-layout` supplies words and lines with
their positions, and a small segmenter turns lines into paragraphs: it drops
lines repeated at the same position on most pages (running heads, folios),
joins a line-final hyphen to the next line's first word, and starts a
paragraph at an indented line or after a vertical gap. **`pdftotext` is a
measurement-only dependency**, run as a child process; whether shipping reads
PDF text through poppler, a Rust crate or recognition of its own is the
reader phase's decision, and the Cambridge Emma, which has no text layer at
all, is why it cannot be assumed.

## 5. The dynamic program: `taiyaku-core::book_align`

Pure, as all of `taiyaku-core` is, so it compiles to WASM like `solve` and
needs no model: embeddings arrive as vectors.

It aligns two block sequences in reading order. A step consumes *i* blocks of
A and *j* of B, for *i*, *j* in `0..=max_group` and not both zero; `(1, 0)` and
`(0, 1)` are text one side lacks. **`max_group` is a parameter**, run at 2, 3
and 4 — that sweep is how question 3 measures "looser". The cost of a step is
the scorer's cost for the two groups; a one-sided step costs a fixed penalty
per scorer, and that penalty is swept and the sweep reported rather than
chosen here.

**A band, reported rather than trusted.** A full matrix is ~5,000 × 5,000
blocks for a long novel *(a guess: Crime and Punishment's eighteen measured
chapters run 44 to 219 paragraphs)*, and each cell tries up to `max_group²`
moves. The search is limited to a band around the length-proportional
diagonal, and the report states **whether any key link falls outside the
band**, so a band too narrow shows up as a figure rather than as silently
wrong links.

**Not `solve`, and not two levels.** `solve` is written for `PageHash`es and
one-to-one pairs with gaps, and it stays untouched. Aligning chapters first and
paragraphs within them was rejected by the survey: Ready's missing Part Three
VI would put every chapter after it against the wrong one.

## 6. Three scorers

All three plug into one `Scorer` trait, so the dynamic program and the report
are shared and only the cost differs.

1. **Length.** Gale and Church's cost: two groups match when their character
   lengths fit the pair's ratio. **The ratio and its spread are fitted per
   pair**, from the pair's own totals, not set to a textbook value — Korean
   runs at 0.62–0.75 of English and the two Crime and Punishment translations
   at about 1.0.
2. **Length and anchors.** Length, plus features that cost no model: numbers
   that appear on both sides, the punctuation profile (quotation marks,
   question and exclamation marks), whether a paragraph opens with dialogue,
   and **for same-language pairs only**, overlap of content words.
3. **Multilingual embeddings.** multilingual-e5-small (MIT, 118 million
   parameters) first, LaBSE (Apache-2.0, 471 million) only if e5 falls short
   — both figures from the models' Hugging Face listings. Vectors
   are computed in `taiyaku-cli`, which already runs an ONNX model through
   `tract-onnx`, with the `tokenizers` crate; the weights are fetched by a
   script beside `fetch-model.sh` and never committed, and the script names
   its source and licence as that one does. A group's vector is the embedding
   of its concatenated text, as vecalign does, cached per run of blocks.
   **Whether `tract-onnx` runs either model is not known** — the research
   behind this design did not check it — so it is the first thing PR 4 does,
   and `ort` is the fallback if it cannot.

## 7. What is reported

Per pair, per scorer, per `max_group`:

1. **Landing accuracy — the reader's own measure.** For each keyed block on
   either side: does the alignment put it inside its true counterpart group,
   which is what a swap would land on? When it does not, by how many blocks
   is it off? One paragraph off is a reader scrolling a line; a chapter off is
   a broken feature, and a single accuracy figure would hide the difference,
   so the distance is reported as a distribution.
2. **Link F1**, strict (the group matches exactly) and lax (the groups
   overlap), in Bertalign's convention, so the figures sit beside published
   ones.
3. **The key's own group sizes** — how many of its links are 1:1, 1:2, 2:3 and
   so on — which answers question 3 from the ground truth before any aligner
   has run.
4. **Cost.** Time and peak memory per book per scorer on the workstation.
   **The NAS is measured only if embeddings turn out to be needed**: the
   other two scorers are arithmetic on string lengths, and no NAS figure for
   them could change a decision.
5. **For the PDF pair**: the text similarity of each OCR paragraph to its keyed
   EPUB paragraph — the same text, so the gap is recognition and segmentation
   noise — and the count of key links that page breaks split into two.

Anna Karenina has no key, so it reports only what needs none: the aligned
paragraph-count ratio per chapter, the share of one-sided steps, and whether
its chapter boundaries fall on 1:1 links.

## 8. Testing the instrument

Four failures this project has already paid for, each with its guard:

- **A scorer can pass against its own defect.** Before any figure is read,
  two controls run on every key: the key scored against itself must give 1.0
  on every metric, and the key shifted by one block must score strictly lower
  on every metric, both figures printed. A metric that does not drop for the
  shifted key cannot see a misalignment, and the run stops there.
- **A fixture where right and wrong look the same proves nothing.** The
  dynamic program's unit fixtures are built so the wrong answer differs from
  the right one: a 1:2 split that a program limited to 1:1 misplaces, a block
  one side lacks, and a paragraph posing as a heading that an aligner trusting
  structure would follow. Each is hand-mutated and watched going red.
- **Extraction has to be tested on the shapes the survey found**, as small
  EPUBs built in the test: verse in calibre's table rows, notes linked both
  ways, notes linked one way from a superscript, line numbers as text, page
  anchors, a `<p>` in heading clothes, and a spine path with `../` in it.
- **A free key that looks complete may not be.** The counts in §3 are
  asserted, since the Beowulf class change halves a naive extraction without
  any error.

## 9. Branches

Each is one checkable claim, squash-merged as this remote requires:

| PR | Claim | Rough size |
| --- | --- | --- |
| 1 | `taiyaku-read::book`: an EPUB becomes blocks, with note bodies out of the flow, correct on the fixtures | ~1 day |
| 2 | `taiyaku-core::book_align` and the length and anchor scorers, correct on the fixtures | ~1 day |
| 3 | The keys (spot-checked by the user), the free-key extractors, `book_experiment`, and the text scorers' results in `docs/notes.md` | 1–2 days |
| 4 | The embedding scorer and its results, and the NAS figure if it wins | ~1 day |
| 5 | The PDF path and the Gatsby results | ~1 day |

*The sizes are guesses.* PR 3's results decide whether PR 4 is needed at all
for the same-language pairs; it is still run, since the cross-language pairs
are expected to need it.

## 10. What waits

- **The reader**: rendering, pagination, notes shown as pop-ups or in the
  margin, the reading-position model — designed after this, from its tables.
- **A sidecar format for books**, and hand-correction of their alignments.
- **Line-keyed commentary** (Longman's Paradise Lost, Arden's Hamlet) and
  apparatus tiers.
- **Recognition for a PDF without a text layer**, such as the Cambridge Emma.
- **Sentence-level alignment** inside matched paragraphs, which question 3's
  answer decides whether anyone needs.

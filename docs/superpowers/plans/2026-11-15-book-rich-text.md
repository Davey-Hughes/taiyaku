# Books rich text Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The book reader shows furigana, italics, bold, superscript and subscript, images, and footnotes and links, without changing any block's text or the number of blocks.

**Architecture:** `taiyaku-read` records formatting as byte ranges ("marks") beside each block's unchanged `text`, and images between blocks as "figures". `GET /api/works/{work}/book` sends both, in UTF-16 units, and a new route serves a book's images. The web reader adds marks as one more layer of cuts in `pieces()`, and draws them in the blocks, compare columns, interlinear line, peek and a footnote popup.

**Tech Stack:** Rust (roxmltree, axum, serde_json), TypeScript/React, vitest (node, jsdom, browser projects).

**Spec:** `docs/superpowers/specs/2026-11-15-book-rich-text-design.md`

## Global Constraints

- A block's `text`, `kind`, `line` and the number of blocks stay identical for every EPUB. Checked by Task 1's snapshot after every Rust task.
- Ruby readings, footnote bodies and images never go into `text` and never become blocks.
- Wire offsets are UTF-16 units, as `sentences` and note runs already are.
- A book never fails to read because of its CSS, a link or an image: those are left out, never errors.
- Italic from `i`, `em`, `cite`; bold from `b`, `strong`; CSS: a selector `.c` or `tag.c`, or an element's `style`, setting `font-style: italic` or `font-weight: bold | bolder | 600–900`; `font-style: normal` ends italic inside it.
- Code comments carry a reason, never a restatement of the code.
- **Deviation from the spec, to confirm with the user:** the furigana switch is a Text setting (`BookTextPrefs.furigana`) like every other text setting, not stored per book.

## Review Focus

1. **Selecting text in a block with marks.** A selection over italic text, a ruby base or a link must give the same `{start, end}` as over the same text without marks. A reading in `<rt>` counts for nothing. Test in Task 8.
2. **An image loading after the page is drawn.** It must not move the reader off the sentence they were on, in scroll or paged layout. Test in Task 10.
3. **A link to a fragment no block holds,** or to a block `numbered()` dropped (a bare verse number). It goes to the next block, or is no mark, and never panics. Test in Task 5.
4. **A stylesheet that is missing, not UTF-8 or not CSS.** The book reads, just without those marks. Test in Task 4.
5. **Overlapping marks:** a link around `<sup>` around `<i>`, bold inside italic, a ruby base cut by a sentence end. Each draws with no base character lost and the reading drawn once. Test in Task 8.

---

### Task 1: Snapshot of every library book's text

**Files:**
- Modify: `crates/taiyaku-read/src/corpus.rs`

**Interfaces:**
- Produces: test `corpus::every_library_book_keeps_its_text`, gated on `TAIYAKU_BOOKS` (a directory) and `TAIYAKU_BOOKS_SNAPSHOT` (a file). It writes the snapshot where the file is absent, and compares where it exists.

- [ ] **Step 1: Write the test.** For every `*.epub` under `TAIYAKU_BOOKS`, recursively, in sorted path order, read it with `read_book`. Record `relative path → sha256 hex of (block count, then each block's kind, line and text, length-prefixed)`. Use `sha2`, already a dependency. A file that fails to read is recorded as `"error"`. Write JSON when the snapshot file is absent. When it exists, assert equality and list every path that differs. Print "skipping" and pass when either variable is unset, as `every_corpus_book_reads` does.
- [ ] **Step 2: Write the snapshot on the unchanged reader.**
  Run: `TAIYAKU_BOOKS="$PWD/taiyaku-library/Books" TAIYAKU_BOOKS_SNAPSHOT="$PWD/target/book-text-snapshot.json" cargo test --release -p taiyaku-read corpus::every_library_book_keeps_its_text -- --nocapture`
  Expected: PASS, and `target/book-text-snapshot.json` has 625 entries.
- [ ] **Step 3: Run it again.** Same command. Expected: PASS (compares equal).
- [ ] **Step 4: Commit** `crates/taiyaku-read/src/corpus.rs`: "Books: a snapshot check that every library book's text and blocks stay as they are".

From here on, "**the snapshot check**" means Step 3's command, and every Rust task runs it before committing.

### Task 2: Marks from inline tags, with offsets through collapsing and the verse-number cut

**Files:**
- Modify: `crates/taiyaku-read/src/book.rs`, `crates/taiyaku-read/src/lib.rs` (re-export `Mark`, `MarkKind`, `LinkTarget`, `Figure`)
- Test: `crates/taiyaku-read/src/book/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Mark { pub range: std::ops::Range<usize>, pub kind: MarkKind } // bytes of `text`
  pub enum MarkKind {
      Italic, Bold, Sup, Sub,
      Ruby { reading: String },
      Link { to: LinkTarget, note: bool },
      Image { src: String, alt: String },
  }
  pub enum LinkTarget { Block(usize), Url(String) } // Block: index into Book::blocks
  Block { …, pub marks: Vec<Mark> }  // sorted by range.start, then longer first
  ```
  All derive `Debug, Clone, PartialEq, Eq`.

- [ ] **Step 1: Write the failing tests.**
  - `inline_tags_mark_their_text`: `<p>A <i>big</i> <b>bold</b> x<sup>2</sup> H<sub>2</sub>O <em>e</em> <strong>s</strong> <cite>c</cite></p>` gives text `"A big bold x2 H2O e s c"`, with Italic on `"big"`, `"e"` and `"c"`, Bold on `"bold"` and `"s"`, Sup on `"2"` (first) and Sub on `"2"` (second), each `range` sliced from `text` equal to that word.
  - `marks_survive_collapsing_whitespace`: `<p>  one\n <i> two  three </i>  four</p>` gives text `"one two three four"`, and Italic slices to `"two three"`. Leading and trailing collapsed spaces are not inside a mark.
  - `marks_follow_the_verse_number_cut`: a poem fixture where `opening_number` takes `"5 Sing <i>goddess</i>"`. The block's `text` is `"Sing goddess"`, `line == Some(5)`, and Italic slices to `"goddess"`.
  - `nested_marks_are_each_kept`: `<p><b>a <i>b</i></b></p>` gives Bold over `"a b"` and Italic over `"b"`.
  - `a_mark_over_nothing_is_dropped`: `<p>x<i> </i>y</p>` has no marks.
- [ ] **Step 2: Run them to verify they fail.** Run: `cargo test -p taiyaku-read book::tests::` Expected: compile errors on `marks`.
- [ ] **Step 3: Implement.** `Walk` records each mark as a raw byte range in `self.text` (opened on entering the element, closed on leaving). It does not split a mark across a block boundary; a mark crossing one is cut at the flush. `collapse` becomes `collapse_mapped(&str) -> (String, Vec<usize>)`, where the second value maps each raw byte offset to its collapsed offset. `flush` maps the marks and drops empty ones. `numbered` shifts and clips the marks of a block whose opening number it cuts, and drops those of a block it removes. It also returns, for later tasks, `Vec<usize>` mapping each input block index to its output index (a removed block maps to the next kept one).
- [ ] **Step 4: Run the tests, then the whole crate.** Run: `cargo test -p taiyaku-read` Expected: PASS.
- [ ] **Step 5: Run the snapshot check.** Expected: PASS.
- [ ] **Step 6: Commit:** "Books: italic, bold, superscript and subscript read as marks beside a block's text".

### Task 3: Ruby as a mark

**Files:** Modify `crates/taiyaku-read/src/book.rs`; Test `crates/taiyaku-read/src/book/tests.rs`

**Interfaces:** Consumes `MarkKind::Ruby { reading }` from Task 2.

- [ ] **Step 1: Write the failing tests.**
  - `ruby_marks_its_base_with_its_reading`: `<p><ruby>坊<rp>(</rp><rt>ぼっ</rt><rp>)</rp></ruby>ちゃん</p>` gives text `"坊ちゃん"`, unchanged from today, and one Ruby mark slicing to `"坊"` with reading `"ぼっ"`.
  - `each_base_in_one_ruby_has_its_own_reading`: `<ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby>` gives two marks: `"漢"`/`"かん"` and `"字"`/`"じ"`.
  - `rb_elements_are_bases`: `<ruby><rb>東京</rb><rt>とうきょう</rt></ruby>` gives `"東京"`/`"とうきょう"`.
  - `a_ruby_with_no_reading_is_no_mark`: `<ruby>字<rt></rt></ruby>` gives no mark.
- [ ] **Step 2: Run them to verify they fail.**
- [ ] **Step 3: Implement** in `Walk::within`, by handling `ruby` before `SKIPPED_ELEMENTS`. `rt` and `rp` stay skipped as text. The base is the text read since the ruby opened or since the last `rt`, and the reading is the `rt`'s text collapsed.
- [ ] **Step 4: Run** `cargo test -p taiyaku-read`, **then the snapshot check.** Expected: both PASS. *Botchan*'s entry must be unchanged.
- [ ] **Step 5: Commit:** "Books: furigana read as a ruby mark over its base".

### Task 4: Italic and bold from CSS

**Files:**
- Create: `crates/taiyaku-read/src/book/styles.rs` (`mod styles;` in `book.rs`)
- Modify: `crates/taiyaku-read/src/book.rs`
- Test: `crates/taiyaku-read/src/book/styles/tests.rs`, `crates/taiyaku-read/src/book/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub(super) struct Styles { /* (tag or None, class) → Style */ }
  #[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
  pub(super) struct Style { pub italic: Option<bool>, pub bold: Option<bool> }
  impl Styles {
      pub fn parse(css: &str) -> Self;                 // never fails; skips what it can't read
      pub fn extend(&mut self, other: Styles);
      pub fn of(&self, tag: &str, classes: &str, style_attr: Option<&str>) -> Style;
  }
  ```
  A later declaration wins, and `style_attr` wins over classes.

- [ ] **Step 1: Write the failing unit tests** in `styles/tests.rs`:
  - `.it { font-style: italic }` gives `of("span", "x it", None).italic == Some(true)`.
  - `span.b { font-weight: 700 }` gives `of("span","b",None).bold == Some(true)` and `of("p","b",None).bold == None`.
  - `font-weight: bolder` and `600` are bold. `400`, `normal` and `500` give `Some(false)`.
  - `.n { font-style: normal }` gives `Some(false)`.
  - `p .it`, `[class=it]`, `.it:first-child`, `.a.b` are ignored, so `of` returns the default.
  - Comments, `@media { … }` blocks, `!important` and upper case (`FONT-STYLE: ITALIC`) are handled. `@media` contents are ignored.
  - `of("span","",Some("font-style: italic; color: red")).italic == Some(true)`.
  - Garbage (`"}{;;: {"`) parses to an empty `Styles` with no panic.
- [ ] **Step 2: Write the failing book tests:**
  - `a_class_a_stylesheet_sets_italic_marks_italic`: a linked `style.css` (`<link rel="stylesheet" href="../css/style.css"/>`, resolved against the document's directory) plus `<span class="it">word</span>` gives Italic on `"word"`.
  - `an_inline_style_element_counts`: the same via `<style>` in `<head>`.
  - `normal_inside_italic_ends_it`: `<i>a <span style="font-style:normal">b</span> c</i>` gives Italic on `"a"` and on `"c"`, not on `"b"`.
  - `a_missing_or_unreadable_stylesheet_leaves_the_book_readable`: a link to an absent file, and one to non-UTF-8 bytes. The book reads with the same text and no CSS marks.
- [ ] **Step 3: Run them to verify they fail.**
- [ ] **Step 4: Implement** `styles.rs`. It is a hand-written scanner, with no new dependency, meeting only the cases above. In `read_archive`, gather each spine document's `Styles` from its `<link rel~="stylesheet">` entries (read with `entry_text`, which ignores errors) and its `<style>` elements, caching parsed sheets by path. `Walk` keeps a stack of the current italic and bold state: an element's `Style` sets it, tags set it to true, and a mark opens or closes where the state changes.
- [ ] **Step 5: Run** `cargo test -p taiyaku-read`, **then the snapshot check.** Expected: PASS.
- [ ] **Step 6: Commit:** "Books: italic and bold read from a stylesheet's classes and an element's style".

### Task 5: Links and footnote references

**Files:** Modify `crates/taiyaku-read/src/book.rs`; Test `crates/taiyaku-read/src/book/tests.rs`

**Interfaces:** Consumes `MarkKind::Link { to, note }` and `LinkTarget` (Task 2), and the block index map from `numbered` (Task 2).

- [ ] **Step 1: Write the failing tests** (two-document fixtures with `text.xhtml` and `notes.xhtml`):
  - `a_noteref_links_to_its_note_block_and_is_a_note`: `<a epub:type="noteref" href="notes.xhtml#n1">1</a>` against `<aside id="n1" epub:type="footnote"><p>The note.</p></aside>` gives `Link { to: Block(i), note: true }` over `"1"`, where `blocks[i].text == "The note."`.
  - `a_sup_link_is_a_note`: `<sup><a href="notes.xhtml#n2">2</a></sup>` and `<a href="#n3"><sup>3</sup></a>` both give `note: true`.
  - `a_link_into_an_endnote_section_is_a_note`: a plain `<a href="notes.xhtml#e1">` whose target sits inside `<section epub:type="endnotes">` gives `note: true`.
  - `a_plain_in_book_link_is_not_a_note`: `<a href="text.xhtml#ch2">see chapter 2</a>` gives `note: false`, to the block holding `id="ch2"`.
  - `an_id_on_an_inline_element_targets_its_paragraph`: `<p>Lead <span id="x">here</span></p>` gives the target = that paragraph.
  - `a_link_to_a_document_alone_targets_its_first_block`.
  - `a_web_link_is_a_url`: `href="https://example.org/a"` gives `LinkTarget::Url("https://example.org/a")`. `mailto:` gives a Url too. `javascript:` gives no mark.
  - `an_unresolvable_link_is_no_mark`: an unknown fragment, a document outside the spine, and an href climbing out of the archive all give no mark and no error.
  - `a_link_to_a_dropped_verse_number_goes_to_the_next_block`: the target id is on a bare number paragraph that `numbered` removes, so the target is the next kept block.
- [ ] **Step 2: Run them to verify they fail.**
- [ ] **Step 3: Implement.** While walking, record `id → (document, output block index at entry)` for every element with an `id`, and each pending link as `(block, raw range, href, note-ness)`. After all documents are read, resolve hrefs against the linking document's directory with `resolve()` (keeping the fragment apart), map through each document's `numbered` index map, and turn them into marks. An `epub:type` attribute is matched on its local name in the OPS namespace (`http://www.idpf.org/2007/ops`), or with no namespace. Note-ness is a `noteref`, a `sup` inside or around the `a`, or a target inside an element whose `epub:type` contains `footnote`, `endnote`, `endnotes`, `rearnote` or `rearnotes`.
- [ ] **Step 4: Run** `cargo test -p taiyaku-read`, **then the snapshot check.**
- [ ] **Step 5: Commit:** "Books: links read as marks, a footnote reference told apart and joined to its note's block".

### Task 6: Images, between blocks and inside them

**Files:** Modify `crates/taiyaku-read/src/book.rs`; Test `crates/taiyaku-read/src/book/tests.rs`

**Interfaces:**
- Produces: `pub struct Figure { pub before: usize, pub src: String, pub alt: String }` and `Book { …, pub figures: Vec<Figure> }`. `src` is the archive path. `before` is an index into `Book::blocks`, `blocks.len()` after the last.

- [ ] **Step 1: Write the failing tests:**
  - `an_image_alone_between_paragraphs_is_a_figure`: `<p>a</p><div><img src="../img/map.png" alt="Map"/></div><p>b</p>` in `text/doc.xhtml` gives blocks `["a","b"]` (unchanged) and `figures == [Figure { before: 1, src: "img/map.png", alt: "Map" }]`.
  - `an_image_inside_a_paragraph_is_an_inline_mark`: `<p>x <img src="g.png" alt="*"/> y</p>` gives text `"x y"` and an `Image` mark with `range.start == range.end == 2`.
  - `an_svg_image_is_read_by_its_xlink_href`: `<svg><image xlink:href="cover.jpg"/></svg>` alone in a document gives a figure.
  - `a_figure_after_the_last_block_has_before_equal_to_the_block_count`.
  - `an_image_whose_src_climbs_out_is_left_out`.
- [ ] **Step 2: Run them to verify they fail.**
- [ ] **Step 3: Implement.** An `img`, or an SVG `image` (`href` or `xlink:href`), records a pending image at the raw position in the current block. At `flush`, a block whose collapsed text is empty turns its images into figures before the next block index in the book. Otherwise they become zero-width `Image` marks at their mapped positions. Figures are remapped through `numbered` too.
- [ ] **Step 4: Run** `cargo test -p taiyaku-read`, **then the snapshot check.**
- [ ] **Step 5: Commit:** "Books: images read as figures between blocks and marks inside them".

### Task 7: The server sends marks and figures, and serves a book's images

**Files:**
- Modify: `crates/taiyaku-serve/src/book.rs`, `crates/taiyaku-serve/src/routes.rs` (router at :2274, grant table at :2614, narrowing table at :3112, beside `/pages/{index}`), `crates/taiyaku-serve/src/auth/tests.rs:57` list
- Test: `crates/taiyaku-serve/src/book/tests.rs`

**Interfaces:**
- Produces the JSON each later web task reads. Every offset is UTF-16, converted with the existing `utf16_ranges`:
  ```json
  block:   {"text", "heading", "line", "sentences", "marks"?: [
             {"kind":"italic"|"bold"|"sup"|"sub","start":0,"end":3},
             {"kind":"ruby","start":0,"end":1,"reading":"ぼっ"},
             {"kind":"link","start":4,"end":5,"block":812,"note":true},
             {"kind":"link","start":4,"end":5,"url":"https://…","note":false},
             {"kind":"image","start":2,"end":2,"image":7,"alt":"*"}]}
  edition: {"label","language","blocks","figures"?: [{"before":1,"image":3,"alt":"Map"}]}
  ```
  `marks` and `figures` are left out where empty. `image` is the index of `src` in `taiyaku_read::member_page_names(path)`, and an image not listed there is left out.
- Produces the route `GET /api/works/{work}/{edition}/book/images/{index}` (`book::get_book_image`). It refuses exactly as `get_book` does (`view::of_work`, missing work, `refuse_if_cannot_stream`), plus 404 for an edition the view hides or that `is_book` says is no book. It serves `taiyaku_read::member_page(path, index)` with the content type from the name's extension and the same revalidation headers `get_page` sends. It never goes through `CompositeSource`.

- [ ] **Step 1: Write the failing tests:**
  - `a_books_marks_and_figures_are_sent_in_utf16`: a fixture block `"😀 <i>é</i>"`. The italic's `start` is 3 (the emoji is 2 UTF-16 units plus the space), not its byte offset.
  - `a_block_with_no_marks_sends_no_marks_field`, and the same for `figures`.
  - `a_book_image_is_served_by_its_index`: 200, `content-type: image/png`, and bytes equal to the archive's.
  - `a_book_image_past_the_list_is_404`, `a_book_image_of_a_hidden_edition_is_404`, `a_book_image_for_an_account_that_may_not_stream_is_refused` (mirroring `an_account_that_may_not_stream_is_refused_the_text`).
- [ ] **Step 2: Run them to verify they fail.** Run: `cargo test -p taiyaku-serve book::` Expected: FAIL.
- [ ] **Step 3: Implement.** Keep the image name list in `ReadBook` beside `sentences`, so it is read once per file hash. Add the route to every table that lists `/pages/{index}`, choosing the posture `/api/works/{work}/book` has in each (`Filters`, and `NoPages` with its reason). The table completeness tests fail until all are done.
- [ ] **Step 4: Run** `cargo test -p taiyaku-serve`. Expected: PASS, including `every_route_refuses_an_anonymous_request` and `every_route_serving_a_member_s_bytes_makes_the_client_revalidate`.
- [ ] **Step 5: Commit:** "Books: the book route sends marks and figures, and a book's images have a route of their own".

### Task 8: Marks drawn in a block, and the furigana setting

**Files:**
- Modify: `web/src/book.ts` (types), `web/src/bookNotes.ts` (`pieces`, `Segment`), `web/src/BookReader.tsx` (`Sentences`, `runsOf`), `web/src/modules/bookText.ts`, `web/src/styles/book.css`
- Test: `web/tests/bookNotes.test.ts`, `web/tests/BookReader.test.tsx`

**Interfaces:**
- Produces:
  ```ts
  // book.ts
  export type Mark =
    | { kind: 'italic' | 'bold' | 'sup' | 'sub'; start: number; end: number }
    | { kind: 'ruby'; start: number; end: number; reading: string }
    | { kind: 'link'; start: number; end: number; block?: number; url?: string; note: boolean }
    | { kind: 'image'; start: number; end: number; image: number; alt: string }
  export interface Figure { before: number; image: number; alt: string }
  Block.marks?: Mark[];  Edition.figures?: Figure[]
  export function bookImageUrl(work: string, edition: string, image: number): string
  // bookNotes.ts
  Segment { text: string; note?: Note; marks: Mark[]; images?: Mark[] /* zero-width, drawn before text */; rubyStart?: boolean }
  pieces(text, sentences, highlights, marks: Mark[] = []): Piece[]
  // BookReader.tsx
  function Runs({ runs, work, edition, furigana }): ReactNode  // replaces runsOf
  // bookText.ts
  BookTextPrefs.furigana: boolean  // default true
  ```
- [ ] **Step 1: Write the failing node tests** in `bookNotes.test.ts`:
  - `pieces cuts at mark boundaries`: text `"a big dog"` with italic 2–5 gives runs `"a "`, `"big"` (italic), `" dog"`.
  - `a highlight over half an italic word keeps both`: highlight 3–9 over italic 2–5 gives `"b"` (italic), `"ig"` (italic plus note), `" dog"` (note).
  - `a ruby base cut by a sentence end marks only its first run rubyStart`.
  - `a zero-width image mark rides on the run after it`, or on an empty run at the end of the text.
  - `pieces with no marks gives what it gave before` (the existing tests stay unchanged).
- [ ] **Step 2: Write the failing jsdom tests** in `BookReader.test.tsx`:
  - An italic and bold block renders `<i>` and `<b>`. A ruby renders `<ruby>坊<rt data-aside>ぼっ</rt></ruby>`.
  - **Review Focus 1:** with a ruby, an italic and a link in a block, selecting `"ちゃん"` gives `selectionIn(...)` `{start: 1, end: 4}`, the same offsets as the block without marks.
  - With `furigana: false`, no `<rt>` is drawn and the base text is unchanged.
  - **Review Focus 5:** `<a>` around `<sup>` around `<i>` renders all three nested, the text whole.
  - An inline image renders `<img src={bookImageUrl(...)} alt>` with no text node.
- [ ] **Step 3: Run them to verify they fail.** Run: `pnpm --dir web vitest run --project node tests/bookNotes.test.ts` and `pnpm --dir web vitest run --project jsdom tests/BookReader.test.tsx` Expected: FAIL.
- [ ] **Step 4: Implement.** Wrap in a fixed order, outermost first: link, bold, italic, sup, sub, then ruby. The reading is drawn only on a run with `rubyStart`. `rt` carries `data-aside` so `wordNodes` skips it, and the CSS sets `rt { user-select: none }`. The furigana setting gets its Settings control the same way `justify` does.
- [ ] **Step 5: Run both commands.** Expected: PASS.
- [ ] **Step 6: Commit:** "Books reader: italic, bold, furigana and inline images drawn from marks".

### Task 9: Marks in the compare columns, the interlinear line and the peek

**Files:** Modify `web/src/book.ts`, `web/src/BookReader.tsx` (the `peeked` memo :316, the `under` memo :383, `Interlinear` :1438, `Peek` :1470); Test `web/tests/book.test.ts`, `web/tests/BookReader.test.tsx`

**Interfaces:**
- Produces:
  ```ts
  export interface Excerpt { text: string; marks: Mark[] }
  export function excerpt(block: Block, [from, to]: [number, number]): Excerpt  // marks clipped, shifted to 0; zero-width kept when from <= start <= to
  export function sentenceExcerpts(edition: Edition): Excerpt[]                // as sentenceTexts
  export function excerptsIn(edition: Edition, range: [number, number]): Excerpt[]  // as sentencesIn
  ```
  `sentenceTexts` and `sentencesIn` stay for read aloud and other callers.
- [ ] **Step 1: Write the failing tests:**
  - `excerpt clips and shifts marks`: block `"One two. Three"` with italic 4–11 and range `[9, 14]` gives `{text: "Three", marks: [{italic, 0, 2}]}`.
  - `sentenceExcerpts(e).map(x => x.text)` equals `sentenceTexts(e)` for the existing fixtures.
  - jsdom: the interlinear line under a sentence draws its ruby. The peek of a block draws its italic. A compare column draws marks (it goes through `Sentences` already, so this only pins it).
- [ ] **Step 2: Run them to verify they fail.**
- [ ] **Step 3: Implement.** `under.texts` and `peeked.texts` become `Excerpt[]`. `Interlinear` and `Peek` draw each excerpt through `pieces(x.text, [], [], x.marks)` and `Runs`, with excerpts joined by `' '`.
- [ ] **Step 4: Run** `pnpm --dir web vitest run --project node tests/book.test.ts` and `pnpm --dir web vitest run --project jsdom tests/BookReader.test.tsx`. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: marks drawn in the peek, the interlinear line and every compare column".

### Task 10: Figures between blocks

**Files:** Modify `web/src/BookReader.tsx` (the block map :431 and the compare row render :533), `web/src/bookPages.ts`, `web/src/styles/book.css`; Test `web/tests/BookReader.test.tsx`, `web/tests/bookPages.browser.test.tsx`

**Interfaces:** Consumes `Edition.figures` and `bookImageUrl` (Task 8).

- [ ] **Step 1: Write the failing tests:**
  - jsdom: a figure `{before: 1}` renders `<figure class="book-figure"><img …></figure>` between block 0 and block 1, with no `data-block`, so `topBlock` and the block numbering skip it. A figure with `before == blocks.length` renders after the last block.
  - jsdom: in compare, a figure renders only in its own edition's column.
  - jsdom: tapping a figure opens it full size in a dialog. Escape closes it.
  - browser (**Review Focus 2**): in scroll and in paged layout, with the reader placed on block 5 and an image above it that loads after first paint (delayed with a test server or a blob URL resolved later), block 5's first sentence is still the top sentence once the image has loaded.
- [ ] **Step 2: Run them to verify they fail.** Run: `pnpm --dir web vitest run --project jsdom tests/BookReader.test.tsx` and `pnpm --dir web vitest run --project browser tests/bookPages.browser.test.tsx`
- [ ] **Step 3: Implement.** Use CSS `max-width: 100%`, and in paged layout `max-height` of the page and `break-inside: avoid`. On each image's `load`, re-anchor with the existing `kept()`/`show()` pair, from the anchor taken before the load.
- [ ] **Step 4: Run both commands.** Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a book's images drawn where it places them, without moving the reader's place".

### Task 11: Footnote popup, in-book links with a way back, web links

**Files:**
- Create: `web/src/BookFootnote.tsx`
- Modify: `web/src/BookReader.tsx` (`onPointerUp` :1008, the bars), `web/src/styles/book.css`
- Test: `web/tests/BookFootnote.test.tsx`, `web/tests/BookReader.test.tsx`

**Interfaces:**
- Consumes: link marks with `block`/`url`/`note` (Task 8), and the `Runs` renderer.
- Produces: `BookFootnote({ block: Block, work, edition, furigana, onGo: () => void, onClose: () => void })`.
- [ ] **Step 1: Write the failing tests:**
  - A single tap on a `note: true` link opens `BookFootnote` with the target block's text and marks. A double tap on the same link does not also peek. Escape or a tap outside closes it.
  - "Go to note" in the popup scrolls to the block (`show(text, block)`) and shows a "Back" button in the bars. "Back" returns to the anchor taken before the jump (`topOf` → `show`), then hides.
  - A tap on a `note: false` in-book link jumps straight away with the same Back.
  - A web link renders `<a href target="_blank" rel="noopener noreferrer">` and is left to the browser.
  - A link's text is still selectable for a highlight, with a press past `TAP_SLOP_PX` or longer than `TAP_HOLD` selecting as today.
  - A tap on a link inside a compare column opens that column edition's block.
- [ ] **Step 2: Run them to verify they fail.**
- [ ] **Step 3: Implement.** In `onPointerUp`, after the slop and hold check and before the double-tap logic: a target inside `[data-link]` acts at once and clears `lastTap`. The `<a>` for an in-book link has no `href` the browser would follow (`role="link"`, `tabIndex=0`, Enter activates).
- [ ] **Step 4: Run** `pnpm --dir web vitest run --project jsdom tests/BookFootnote.test.tsx tests/BookReader.test.tsx`. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a footnote opens beside its reference, and an in-book link jumps with a way back".

### Task 12: Whole-branch gate and a check on real books

**Files:** Modify `docs/status.md` (the Books entry: what now shows, and an O-number for each thing left out by the spec's non-goals that a reader would notice: tables, lists, block quotes)

- [ ] **Step 1: Run the snapshot check.** Expected: PASS, all 625 unchanged.
- [ ] **Step 2: Run the full gate.** Run: `scripts/check-all.sh > target/check-all.log 2>&1; echo "exit $?"` (watched with Monitor on progress and failure lines). Expected: `exit 0`, or only the corpus checks skipped, as on main.
- [ ] **Step 3: Open real books with the `run` skill:** *Botchan* (furigana on and off, in interlinear with an English edition), a book with CSS-only italics, a book with `noteref` footnotes, and a book with images. Take one screenshot of each.
- [ ] **Step 4: Commit** `docs/status.md`: "Books rich text: status".

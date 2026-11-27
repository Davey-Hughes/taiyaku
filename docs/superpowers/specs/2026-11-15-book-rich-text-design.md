# Books: furigana, italics and bold, images and footnotes — design

**Status: settled in brainstorming on 2026-11-15. Built in PR #253.**

This is **project 1 of 4** of the book reader's gaps, agreed on 2026-11-15:

1. Richer text from the EPUB (this document).
2. Reading comfort: text settings, vertical Japanese, search in a book, bookmarks.
3. Learning: tap a word to look it up, known-word tracking.
4. A Korean dictionary.

All four will be built; this one goes first because vertical text and the text
settings draw on whatever this one draws.

## Why

taiyaku's book reader is built for learning a language and comparing
translations (#249). Today it shows a book as plain text: `taiyaku-read`'s
`Block` holds `text`, a heading flag and a verse line number, and nothing
else (`crates/taiyaku-read/src/book.rs:28`). A ruby's reading is dropped
and its base kept as plain text (`rt` and `rp` are in `SKIPPED_ELEMENTS`,
`book.rs:109`). So a learner loses furigana, a reader loses italics
(thoughts, emphasis, titles, foreign words), illustrations and maps never
appear, and a classic's notes are a run of paragraphs at the back with no
way to reach them from the text.

## What the library holds (measured 2026-11-15 over the test EPUBs)

| Feature | Books |
|---|---|
| `<i>`/`<em>` | 485 |
| italics only through a CSS class (`font-style: italic`, no `<i>`/`<em>`) | 125 |
| `<b>`/`<strong>` | 293 |
| `<img>`/SVG `<image>` | 621 (covers included) |
| a footnote as `<sup><a>` or `<a><sup>` | 100 |
| `epub:type="noteref"` | 37 |
| ruby | 1 (*Botchan*, Wikisource, 3,042 glosses) |

*Botchan* is the only Japanese book, so furigana is tested against it and
against fixtures.

## Goals

- Furigana shown over its base text, with a switch to hide it.
- Italic, bold, superscript and subscript shown as the book sets them, from
  tags and from simple CSS classes.
- Images shown where the book places them, between paragraphs or inside one.
- A footnote reference opens its note in a popup; any other in-book link
  jumps there, with a way back; a web link opens in a new tab.

## Non-goals

- Tables, lists, block quotes, poetry indentation, drop caps, colours, fonts
  and any layout CSS. Those stay paragraphs as today.
- Complex CSS: only a selector that is one class (`.it`) or a tag with one
  class (`span.it`) is read. Descendant, attribute and pseudo selectors are
  not.
- Vertical text and text settings (project 2).
- Changing what the aligner reads.

## Anti-goals: what would make this a failure even if it works

- **Any change to a block's `text` or to the number of blocks**, for any book.
  Notes and highlights (`note.ts`/`note.te`, UTF-16 offsets into `text`),
  sentence ranges, pins (block numbers) and the hand-made alignment keys all
  count in today's text and blocks. A shifted offset silently moves every
  saved highlight. Ruby readings, footnote bodies and images are therefore
  **never** put into `text` and **never** become blocks of their own.
- The alignment results in `docs/notes.md` moving at all.
- A popup or link stealing the double tap that peeks, or the selection that
  makes a note.

## Approaches

1. **Ranges beside the text — chosen.** Each block also carries a list of
   ranges over its existing `text` (italic 12–30, ruby "ぼっ" over 0–1, link to
   block 812), and each edition a list of images anchored to a block. Text and
   blocks stay byte-identical, so notes, pins, keys and alignments are safe.
   The renderer adds one more layer of cuts in `pieces()`. Costs: tables and
   lists stay plain paragraphs.
2. **Cleaned HTML per block.** Keeps any formatting, but every selection,
   highlight and sentence span would need mapping between arbitrary markup and
   plain-text offsets, and the HTML must be sanitised. Fragile where the
   current design is exact.
3. **The EPUB's own documents in a frame** (as epub.js readers do). Most
   faithful look, but drops the block model compare, interlinear, peek and
   swap are built on. Ruled out.

The user chose 1 on 2026-11-15.

## The design

### Reading (`taiyaku-read`)

`Block` gains `marks: Vec<Mark>`, each a range `[start, end)` over `text` and
a kind:

- `Italic`, `Bold`, `Sup`, `Sub`
- `Ruby { reading: String }` — the base is the range; the reading is the
  `rt` text joined (`rp` still skipped)
- `Link { to: LinkTarget, note: bool }`, `LinkTarget` being a block
  (`Block(usize)`, the index into `Book::blocks` of the block holding the
  target `id`, or the next block after it) or an external URL. `note` is
  true where the link is a footnote reference: `epub:type="noteref"`, a
  link in or around a `<sup>`, or a target inside a note. A note is an
  element of `epub:type` footnote, endnote, rearnote or note, or what a
  notes list (`epub:type` footnotes, endnotes, rearnotes or notes) holds,
  except the list itself, the groups it sets its notes in and the headings
  met among those groups. Read down from the list, every `div`, `section`,
  `nav`, `header`, `hgroup`, `ol` or `ul` on the way (`NOTE_GROUPS`) is a
  group whatever it opens on; only a group that is itself a link's target
  is asked whether it opens on a heading (`opens_on_heading`), and is a
  note if it does not. O335 records the shapes this misreads. A link from
  inside a note to a target outside every note is the note's way back, a
  plain jump whatever its markup.
- `Image { src, alt, link }` — zero-width (`start == end`), for an image inside a
  paragraph. `link` is the link the image is set in, if any, an
  `ImageLink { to, note }` read as a `Link`'s is. It rides on the image
  because a link holding only an image covers no text, so no `Link` range
  could hold it; a link whose target cannot be resolved leaves the image
  unlinked.

`Book` gains `figures: Vec<Figure>`, each `{ before: block ordinal in the
book, src, alt, link }` for an image between blocks, `link` as an `Image`'s;
one after the last block has `before == blocks.len()`. `src` is the image's
path in the archive, resolved as `resolve()` resolves hrefs.

Ranges are counted in what `text` holds after collapsing whitespace, removing
soft hyphens and taking off a verse line's printed number, so the walker must
track positions through those same steps. **Offsets are UTF-16 units**, as
`sentences` and note runs already are on the wire (decide on the Rust side
whether to store bytes and convert at the route, or store UTF-16; the wire
format is UTF-16).

Italic and bold come from tags (`i`, `em`, `cite`, `b`, `strong`) and from
the spine documents' stylesheets (linked and inline `<style>`) and `style`
attributes: a stylesheet rule whose selector is `.c` or `tag.c`, or an
element's own `style`, that sets `font-style: italic` or
`font-weight: bold | bolder | 600–900` marks elements of that class.
`font-style: normal` inside an italic run ends it (the usual way an emphasised
word is set inside italic text).

A link whose target cannot be resolved is not a mark. A footnote's marker
text (`1`, `*`) stays in `text` as today.

**Invariant, tested over the whole test library:** for every EPUB, the
blocks' `text`, `kind`, `line` and number are identical before and after this
change. A CLI check reads every book with the old and new reader and compares.

### Server (`taiyaku-serve`, `GET /api/works/{work}/book`)

Each block in the JSON gains `marks`, omitted where empty, and each edition
gains `figures`. A link's target block is sent as its block index in that
edition; an image's or a figure's link is sent as its `link`,
`{ block | url, note }`. An image's `src` is sent as its index into the archive's image
entries sorted by name (`page_names_from`, `taiyaku-read/src/pages.rs`), and
served by a **new route, `GET /api/works/{work}/{edition}/book/images/{index}`**,
with the same access rules as `/api/works/{work}/book` (added to every route
table in `routes.rs` beside it). An image not in that list is left out.

The existing `/pages/{index}` route cannot serve these: for a book edition the
sidecar's `members[0].pages` is its block count (`scan.rs`, `pages_of`), which
pins and library cards rely on, and `Edition::locate` refuses any index past
it. That meaning of `pages` is left as it is; the new route reads the archive
directly and never goes through `CompositeSource`/Stump.

The alignment cache stays keyed as it is; the aligner reads only `text`.

### Reader (web)

- `pieces()` (`web/src/bookNotes.ts`) cuts at mark boundaries as it does at
  sentence and highlight boundaries; `runsOf` (`web/src/BookReader.tsx`) wraps runs in `<i>`, `<b>`,
  `<sup>`, `<sub>`, `<a>`, `<ruby>…<rt>`, and draws inline images.
- **Marks show everywhere text is drawn**: the blocks, the compare columns,
  the interlinear line, the peek and the footnote popup. `sentencesIn` and
  `sentenceTexts` (`web/src/book.ts`) today hand those asides plain strings;
  they must carry each sentence's marks, cut to its range (a sentence can
  cross blocks), and the asides draw through the same run renderer.
- **Furigana** shows by default; a setting in the Text module turns it off.
  Selecting, highlighting and look-up see only the base text (the `rt` is
  not selectable).
- **Figures** draw between blocks at their width, no wider than the column,
  and in compare and interlinear only in the column of their edition. An
  image opens full size on tap, unless it is set in a link: then the tap
  follows the link, as a tap on a linked word does.
  *Note, 2026-11-25: narrowed for a page image, a figure that is its EPUB
  document's whole content, by the owner's rulings of 2026-11-25 (O571, O572
  and O574, the book image pages PR). Paged outside compare, a page image is
  drawn on a page of its own, in place of "at their width": scaled down to
  fit the page always, and up to fit it only while the comic reader's
  upscale-to-fit is on. A tap on it there opens nothing, and only a tap on
  its picture, where it is set in a link, follows the link. Where an image
  still opens full size, the full view follows the same setting.*
- **Footnotes**: tapping a `note` link opens a popup holding the target block
  (with its marks), a link to go there, and closes on Escape or tapping
  outside. The popup is set under its reference in the top half of the screen
  and over it in the bottom half, on the other side where that one has no
  room, and mid-screen where neither has or once the window is resized. Other in-book links jump to the block, and a "back" control
  returns to where the reader was. External links open in a new tab.
- Paged layout's page counting must count figures' heights.

## My calls (the user may overrule)

- Furigana on by default, a toggle to turn it off.
- Simple class selectors only, to reach the 125 CSS-only books without a CSS
  engine.
- Images served by a new book image route, since the pages route counts a
  book's blocks, not its images.
- `style` attributes read as well as class rules: 40 of the 125 CSS-only
  books use both (measured 2026-11-15; none use attributes alone).
- Marks drawn in the peek, interlinear and compare views too, since a learner
  reading the Japanese interlinear needs furigana there most.
- The popup shows one block, the target, not the run of blocks after it.

## Left to the builder

- Where a ruby base or a link crosses a sentence or highlight boundary: the
  renderer may split it, but no base character may be lost, a reading must be
  drawn once, and selection offsets must still map to `text`.
- How the tap on a link is told from the double tap that peeks.
- How the walker tracks offsets through collapsing and the verse-number cut.

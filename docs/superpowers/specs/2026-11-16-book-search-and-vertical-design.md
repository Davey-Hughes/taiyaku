# Books: search in a book, and vertical Japanese — design

**Status: settled in brainstorming on 2026-11-16. Built in PR #260.**

This is **project 2 of 4** of the book reader's gaps. Project 1 (furigana, formatting, images, footnotes) is `2026-11-15-book-rich-text-design.md`, merged as #253, and this builds on it. Of project 2's original four items, two are already done:
- **Text settings** already exist (`web/src/modules/bookText.ts`: size, line spacing, column width, typeface, justify, indent, page colour, and furigana from project 1).
- **Bookmarks:** the user decided a note with no text, written with `n` and listed with `a`, already does the job.

## Why

The reader is for learning a language and comparing translations.
- **Search:** finding every place a name, word or phrase occurs is basic to both. There is no find in a book today.
- **Vertical text:** Japanese books are printed top to bottom, right to left. Reading them that way is part of reading Japanese, and furigana sits beside vertical text as it does in print.

## Goals

1. **Search the book.** It opens from a key or a button and searches the edition being read, or every edition with a switch. Each match is listed with text around it, and pressing a match jumps there with the match lit.
2. **Read a Japanese edition vertically,** in scroll and paged layout, with every existing feature working:
   - furigana, marks, figures and footnotes;
   - highlights and notes, selection, look-up and read aloud;
   - the reader keeping its place, the progress readout, and page turns.

## Non-goals

- A server-side search index.
- Search syntax: no regex, whole-word or case-sensitive options.
- Vertical text in compare or interlinear layout. Those stay horizontal even when the setting is on.
- Vertical text for Chinese or Korean, which only follows if a later project asks for it.
- Bookmarks as their own feature.

## Anti-goals

- **Saved positions or notes moving.** Neither feature changes block text, block numbering or offsets.
- **Search counting a furigana reading as text.** A match must be in the block's own text, as selection is.
- **Vertical layout breaking the reader's place.** Switching to it, resizing, or images loading must not lose the paragraph the reader was on.
- **A Japanese learner meeting vertical text without choosing it.** It is off by default.

## Approaches

### Search
1. **Search in the browser over the book JSON already loaded: chosen.** Every edition's text is in memory (`Book.editions[k].blocks`), and the book is at most a few MB of text. A scan is fast enough, needs no server work, and respects access, since only editions the caller sees are loaded.
2. **A server full-text index.** More work, it duplicates what the browser holds, and it only pays off across books. Ruled out.

### Vertical
1. **Vertical writing in both scroll and paged layout: chosen** (the user agreed on 2026-11-16). The text gets `writing-mode: vertical-rl`, and `web/src/bookPages.ts` learns the vertical axes. Paged layout is how most reading on a screen is done.
2. **Scroll layout only.** About half the work, but it leaves out the main way of reading.
3. **Rotating a horizontal layout.** Wrong for CJK glyphs, punctuation and ruby. Ruled out.

## The design

### Search (web only)

**Opening it**
- A key, `/`, which is free in the book reader's bindings (verify against `web/src/modules/*` and the global reader keys).
- A button in the bars.
- It opens a search panel like the existing panels (notes, contents).

**Matching**
- A substring match over each block's `text`, after NFKC and case folding on both sides (Settled 2), so full-width and half-width forms match and case does not count.
- Ruby readings are not searched, since they are not in `text`.
- No minimum length, but the search waits for more input until the query holds 2 characters, or 1 where it holds a letter of a CJK script (Han, hiragana, katakana, Hangul or Bopomofo), so one kanji or one Hangul syllable is a query. Characters are counted as code points, each once however it folds (ß, which folds to ss, is one, and e with a combining accent is one), white space counts for nothing, and punctuation the CJK scripts share (・, 「) is not a letter.
- Searching runs in chunks or after a short debounce, so typing never blocks.

**Results**
- Grouped by chapter, using the existing contents `Entry` list.
- Each shows about 30 characters either side with the match in bold. A count shows at the top, capped in display at 500 ("500+"), with every match still reachable through next/previous.
- The scope switch searches every edition. Results then carry their edition's label, and pressing one swaps to that edition at the match.

**Jumping**
- Pressing a result places the reader on that block at the match's offset (existing `show(text, block, at)`), in compare by that character in the first column, and lights the match with a temporary highlight-like mark. That mark is not a note and is never saved.
- Enter and Shift+Enter in the box, or `]` and `[` after closing it, step to the match after or before the one last gone to, round from either end.
- When the match last gone to is no longer in the list (the query or the edition changed), Enter and `]` go to the first match at or after the reader's place, and Shift+Enter and `[` to the last match before it. The first search after opening still steps from the book's first match.
- Escape closes the panel and keeps the light until the next tap.

**Layouts**
- In compare, search runs on the edition in the first column.
- In interlinear, it runs on the main line.

### Vertical Japanese (web only)

**The setting**
- `BookTextPrefs.vertical: boolean`, default `false`, in the Text module, labelled like the others.
- It applies only when the edition shown is Japanese (Settled 1).
- In compare or interlinear layout it has no effect.

**Drawing**
- `.book-text[data-vertical]` sets `writing-mode: vertical-rl` and `text-orientation: mixed`.
- Paragraph indent, block spacing and figure sizing use logical properties (`margin-block`, `padding-inline-start`, `max-inline-size`, `max-block-size`), so they hold in both modes.
- Line numbers and note marks, which are absolutely positioned, use logical insets. A note's mark sits out from its block by 0.9 of the text's size, beside a heading too, so the text's side padding is at least the text's size (`max(var(--space-4), 1em)`, paged `max(var(--space-6), 1em)`) and the mark is never clipped. Horizontal text has the same padding: scrolled, 18 px at the default size, where `--space-4` alone is 16 px.
- A number of one or two ASCII digits that touches no other digit in the block's text (a full-width digit counts as one) is set upright across its line, as a book prints it: `pieces()` finds each number in the block's whole text and marks on each run the part of it that run holds, and `Runs` draws that part in a `.book-upright` span of its own with `text-combine-upright: all`, which combines all of an element's text. The span sits inside every mark, a ruby's base and a link included, and a number cuts no run, so the ruby's reading still covers its whole base and the link stays one element. Only the drawing changes: the block's text and the offsets of a search, a note and a selection are unchanged. Only the main text is drawn so, not a peek (O357) or a footnote, and a number with a run's edge inside it, a highlight's, a match's, a mark's or a sentence's, draws its digits stacked (O356).
- Ruby sits to the right of its base, as browsers do natively.

**Scroll layout, vertical**
- The text overflows sideways and is read right to left, starting at the right edge.
- `scrollLeft` is 0 at the start and grows negative going left, as the spec and current Chrome and Firefox have it. The builder confirms this in the browser test project.

**Paged layout, vertical**
- CSS columns in a vertical writing mode stack in the inline direction, so pages stack downward and turn by `scrollTop` in steps of `clientHeight`.
- The builder confirms the actual multicol behaviour in Chromium first, and picks the axis it really produces.

**Changes to `bookPages.ts`**

Every function that reads `scrollLeft`, `scrollTop`, `clientWidth`, `getBoundingClientRect().left` or `.top` becomes axis-aware, through one helper describing the current flow:

| Mode | Block axis | Page axis |
|---|---|---|
| Horizontal scrolled | down | — |
| Horizontal paged | down | rightward |
| Vertical scrolled | leftward | — |
| Vertical paged | leftward | downward (or as measured) |

The functions affected:
- `topBlock`: "past the start" is past the right edge in vertical.
- `topOf`, `shows`, `kept`, `show`, `charRect`'s use.
- `pagesOf`, `turn`, `pageTurnedBy`.
- `movesAnchor`.

**Page turns in vertical**
- Paged, a tap on the left third turns forward and the right third turns back, the reverse of horizontal, as a Japanese book's pages run.
- Arrow keys follow the same rule, scrolled too: ← forward, → back.
- The existing step intents (`step` by ±1) keep their meaning of forward and back. Only the mapping from key or side to step flips.

**Switching**
- Turning vertical on or off re-anchors as resizing does: the anchor taken before the change is shown after it (`show`).
- Swapping editions, between a Japanese and a non-Japanese one too, goes to the counterpart of the block at the top in the other edition, or, from a search match, to the match's own place: an offset into one edition's text names nothing in another's.
- The anchor (`anchorOf`) is `topOf`'s character, or the figure the screen starts in, which `topOf` passes over (`Anchor.figure`), and may carry how far into the anchor the screen starts (`Anchor.into`). A docked panel's set-in, which redraws the same boxes at another width, keeps `into`, and so does the re-place after an image loads, paged. Every other re-place sets the anchor from its start, since a fraction means nothing against a box of another size or kind. So outside compare, after a re-place the reader is on the same page, paged, or at the same first line, scrolled, and a figure at the top stays at the top. Compared, the anchor is placed through the row holding it; `kept` and `show` in `bookPages.ts` say how. An image loading in scrolled text places nothing: the browser's scroll anchoring holds the line (O353).
- A docked panel sets the text in beside it in the sidebar layout (`(width >= 900px) and (height >= 500px)`), so a match stepped to with the search open is never under it, and opening or closing one there re-anchors too. In the phone layout the panel goes over the text, which is not laid out again, so nothing is placed again. In vertical text the panels dock on the left, the end the text is read towards.
- A Settings Reset keeps the anchor as a change to a setting does. One that changes no layout, a section that lays nothing out (Motion, Keyboard help, Read aloud) or one already at its defaults, moves nothing.

## Calls made for the user (may be overruled)
- `/` opens search.
- Search is case-insensitive and NFKC-folded.
- Results are grouped by chapter, and the display is capped at 500.
- The search light is temporary and never saved.
- Vertical is off by default and applies only to Japanese editions.
- In vertical, ← turns forward, and paged, so does a tap on the left third.

## Settled after the builder check (my calls)

1. **What counts as Japanese.** An edition is Japanese when the first subtag of `Edition.language` (split at `-` or `_`, case ignored) is `ja` or `jpn`; any other, `jav` or `jam` among them, is not. Where `language` is null, empty or `und`, it is Japanese when more than half of the letters in its first 50 blocks are kana or kanji and some of them are kana, so an all-kanji Chinese text is not. There is no per-edition override.
2. **Folding and offsets.** Search folds a run at a time, a base and the marks after it (ｶ and ﾞ fold together to ガ): NFKC, then lower case, then ß to ss, ς to σ, ı and i with a combining dot above to i, and a variation selector to nothing. It keeps a map from each folded character back to the UTF-16 offset in `text` where its run begins, and a match that ends inside a run's folded form takes the whole run. Every offset it reports (for the list, the jump and the light) is in the original `text`. This is required: a fold that changes length (`㈱`, half-width kana with a voicing mark) must still light exactly the original characters.
3. **The match light.** It is a transient run in `pieces()`: a new optional `found` range alongside highlights. It is cut and drawn like a highlight, as `<mark class="book-found">`, and is never a note. It crosses marks, ruby and links the same way highlights do.
4. **The column-width setting in vertical** caps line length, which in vertical is the column's height. The paged container caps its height with `max-inline-size`. If Chromium's multicol stacks vertical pages in a different direction from the one guessed above, the design holds and only the page axis in the helper changes.
5. **Layout switches re-anchor too.** Changing between scroll, paged, compare and interlinear, toggling vertical, and opening or closing a docked panel where it sets the text in all keep the anchor taken before the change and `show` it after (Switching, above), as a resize and a Settings Reset do. Swapping editions goes to the counterpart of the block at the top instead, or to a search match's own place. Compare or interlinear on a Japanese edition with vertical on draw horizontally.
6. **Page-turn flipping belongs to the book reader.** The book reader does not use `modules/direction.ts` (the page reader's right-to-left setting). Vertical flips the side and arrow mapping inside the book reader only, with no interaction with a work's `rtl` setting.
7. **Stepping through matches from the keyboard:** `]` next and `[` previous, which are free. `n` stays new note.
8. **Chapters across editions.** `contents(blocks)` is per edition, so each result is grouped under its own edition's chapter. With every edition searched, results are grouped by edition first, then by chapter. The 500 display cap applies to the whole list.

## Left to the builder
- The exact panel layout, following the existing panels' look.
- The debounce or chunking figure.
- Where the search button sits in the bars.
- The measured scroll and multicol axes in Chromium, confirmed by a browser test before the `bookPages` changes are written against them.

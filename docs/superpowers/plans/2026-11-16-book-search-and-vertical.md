# Books search and vertical Japanese Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The book reader finds text in a book (one edition or all), and draws a Japanese edition vertically in scroll and paged layout.

**Architecture:** Both features are web only.
- Search is a pure module that folds the text and searches over the book JSON already in memory, plus a panel drawn like Contents. A match is lit by a transient `found` range in `pieces()`.
- Vertical is a Text setting that puts `data-vertical` and `writing-mode: vertical-rl` on `.book-text`. `bookPages.ts` routes every measurement through one axis helper, so place-keeping, pages and turns work in all four flows.

**Tech Stack:** TypeScript/React, CSS logical properties, vitest (node, jsdom, browser projects).

**Spec:** `docs/superpowers/specs/2026-11-16-book-search-and-vertical-design.md`

## Global Constraints

- **Nothing moves.** No change to block text, block numbering, offsets or saved notes.
- **The search light is never a note** and is never saved.
- **Matches and offsets** are UTF-16 offsets into the original `text`, even after NFKC folding. Ruby readings are never searched.
- **Japanese:** `Edition.language` starts with `ja` (ignoring case). Where `language` is null, more than half of the letters in its first 50 blocks are kana or kanji.
- **Vertical:** `BookTextPrefs.vertical`, default `false`. It has no effect in compare or interlinear layout.
- **Keys:** `/` opens search, `]` steps to the next match and `[` to the previous. `n` stays new note.
- **Vertical page turns:** the left third of the screen and ← turn forward, the right third and → turn back. Step intents keep meaning forward (+1) and back (−1). The book reader does not use `modules/direction.ts`.
- **Re-anchoring:** toggling vertical, swapping editions and changing layout each take `topOf` before the change and `show` after.
- **Tooling:** pnpm is not on PATH. Run tools from `web/` through `node_modules/.bin` (`vitest run --project node|jsdom|browser <file>`, `tsc -b`, `biome check ./src ./tests`).
- **Comments** give a reason, never a restatement. Doc comments are at most 8 lines (the gate's comment guard).

## Review Focus

1. **A query in one width finds text in the other.** A half-width kana query (`ｶﾞ`) finds full-width text (`ガ`) and lights exactly that character. The reverse holds too. Test in Task 1.
2. **A search jump lands on the right page** in paged and in vertical paged layout. Test in Task 6.
3. **Turning vertical on mid-book keeps the paragraph.** The block at the top before the switch is the block at the start after it. Test in Task 6.
4. **Selection in vertical text with furigana** gives the same `{start, end}` as in horizontal. Test in Task 4.
5. **Search on a large book stays responsive.** Searching 3 editions of 3,000 blocks each (about 2 MB of text) for a common two-letter query finishes in under 100 ms in node, and the panel shows at most 500 results while counting all. Test in Task 1.

---

### Task 1: Search core

**Files:** Create `web/src/bookSearch.ts`. Test `web/tests/bookSearch.test.ts`.

**Interfaces:**
- Produces:
  ```ts
  export interface Match { edition: string; block: number; start: number; end: number } // UTF-16 in block.text
  export interface Folded { text: string; to: number[] } // to[i] = offset in the original text of folded char i; to[text.length] = original length
  export function fold(text: string): Folded               // NFKC per character, lower-cased
  export function searchBook(book: Book, query: string, editions: string[]): Match[] // in edition order, then block, then start
  export function ready(query: string): boolean            // ≥1 char if it holds kana/kanji, else ≥2
  export function isJapanese(edition: Edition): boolean
  export function context(text: string, m: Match, around = 30): { before: string; match: string; after: string }
  ```
- [ ] **Step 1: Write the failing tests:**
  - `fold('ｶﾞ').text === 'ガ'`. `fold('㈱').text === '(株)'`, with `to` mapping each of the three characters back to 0 and `to[3] === 1`.
  - `searchBook` over one block `'ガイド'` with query `'ｶﾞ'` gives `[{start: 0, end: 1}]`. Text `'ｶﾞｲﾄﾞ'` with query `'ガ'` gives `{start: 0, end: 2}`.
  - Case-insensitive: `'Harry'` matches query `'harry'`.
  - Overlapping occurrences: `'aaa'` / `'aa'` gives starts `[0, 1]`.
  - A match never spans two blocks.
  - `ready('a') === false`, `ready('ab') === true`, `ready('猫') === true`.
  - `isJapanese({language: 'JA-jp'})` is true. With `language: null`, blocks mostly in kana are true and English blocks are false.
  - `context` clips at the block's ends.
  - **Review Focus 5:** build a synthetic book of 3 editions × 3,000 blocks of about 220 characters each. `searchBook(book, 'th', all)` returns in under 100 ms, measured with `performance.now()` and taking the median of 3 runs.
- [ ] **Step 2: Run to verify they fail.** Run `node_modules/.bin/vitest run --project node tests/bookSearch.test.ts`. Expected: FAIL.
- [ ] **Step 3: Implement.** Cache each block's `Folded` in a `WeakMap<Block, Folded>`, so a new query does not fold the book again.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a book's text searched, folded to NFKC with its offsets kept".

### Task 2: The search light in `pieces` and `Runs`

**Files:** Modify `web/src/bookNotes.ts` (`pieces`, `Segment`) and `web/src/BookRuns.tsx`. Test `web/tests/bookNotes.test.ts` and `web/tests/BookReader.test.tsx`.

**Interfaces:**
- Produces `pieces(text, sentences, highlights, marks = [], found?: [number, number]): Piece[]`, with `Segment.found?: true`. `Runs` draws a found run as `<mark class="book-found">`, inside any note `<mark>` and outside the mark wrappers.
- [ ] **Step 1: Write the failing tests:**
  - (node) `pieces('a big dog', [[0, 9]], [], [], [2, 5])` gives runs `'a '`, `'big'` (found), `' dog'`.
  - (node) A found range crossing a highlight and a ruby keeps every base character, and `rubyStart` stays on the first run only.
  - (node) Without `found`, the output equals today's for the existing fixtures.
  - (jsdom) A block drawn with `found` shows `.book-found` holding exactly the match text, and `selectionIn` offsets on that block are unchanged.
- [ ] **Step 2: Run to verify they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the node and jsdom files. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a search match lit as a run of its own, never a note".

### Task 3: The search panel

**Files:**
- Create `web/src/BookSearch.tsx` and `web/src/modules/bookSearch.ts`.
- Modify `web/src/BookReader.tsx` (add `'search'` to `Panel`, add the intent mapping, a bars button, the found state, and `]`/`[`) and `web/src/styles/book.css`.
- Test `web/tests/BookSearch.test.tsx` and `web/tests/BookReader.test.tsx`.

**Interfaces:**
- Consumes Task 1's `searchBook`, `ready`, `context` and `Match`, and Task 2's `found` parameter.
- Produces:
  - `export const TOGGLE_INTENT = 'book-search:toggle'`, `NEXT_INTENT = 'book-search:next'` and `PREVIOUS_INTENT = 'book-search:previous'`.
  - `bookSearchModule` with keys `/`, `]` and `[`. It has no prefs, like `contentsModule`.
  - `BookSearch({ book, edition, entries, onPick: (m: Match) => void, onClose })`.
- [ ] **Step 1: Write the failing tests** (jsdom):
  - `/` opens the panel with focus in the box. Typing `harry` lists results grouped under each chapter's title (from `contents(blocks)` of that edition), each showing before, the match in bold, and after.
  - The count reads `N matches`. Past 500 it reads `500+ matches`, and only 500 rows are drawn.
  - Pressing a result calls `show` on its block at its start and lights it (`.book-found` in the block). The panel closes.
  - Enter and Shift+Enter in the box step to the next and previous match and light each without closing. After closing, `]` and `[` step.
  - The "All editions" switch groups results by edition label, then by chapter. Picking a result from another edition swaps to it at the match.
  - Escape closes the panel and the light stays. The next tap on the text clears it.
  - In compare the search runs on the first column's edition, and in interlinear on the main line.
  - `n` still opens a new note.
  - A query that isn't `ready` lists nothing and shows no count.
- [ ] **Step 2: Run to verify they fail.**
- [ ] **Step 3: Implement.** Debounce input by 120 ms. The help panel picks up the module's keys the way the other modules' do.
- [ ] **Step 4: Run** `vitest --project jsdom tests/BookSearch.test.tsx tests/BookReader.test.tsx`, `tsc -b` and biome. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: find in a book, one edition or all, from `/`".

### Task 4: Vertical Japanese is drawn

**Files:** Modify `web/src/modules/bookText.ts` (`vertical: false`), `web/src/BookReader.tsx` (`data-vertical` on `.book-text`) and `web/src/styles/book.css`. Test `web/tests/BookReader.test.tsx` and create `web/tests/bookVertical.browser.test.tsx`.

**Interfaces:**
- Consumes `isJapanese` (Task 1).
- Produces `.book-text[data-vertical]`, present only when `prefs.vertical && isJapanese(shown edition) && mode is scroll and not compare or interlinear` (paged is a switch on scroll, so it is included).
- [ ] **Step 1: Write the failing tests:**
  - (jsdom) `data-vertical` is set for a `ja` edition with the pref on. It is absent for an `en` edition, absent with the pref off, and absent in compare and in interlinear.
  - (jsdom) The setting appears in Settings like `justify`.
  - (browser) **Axis probe.** In a 400×300 box, horizontal-scrolled `vertical-rl` text with overflow starts at `scrollLeft === 0` and moving toward the end makes `scrollLeft` negative. In paged mode (the same multicol CSS as `.book-text[data-paged]`, with `data-vertical`), record which of `scrollWidth`/`scrollHeight` exceeds the client size. **Assert the measured facts and write them into a comment at the top of the test.** Task 5 codes against them.
  - (browser) **Review Focus 4.** In vertical, a block with ruby `坊/ぼっ` + `ちゃん`, selecting the drawn `ちゃん` text nodes, gives `selectionIn` `{start: 1, end: 4}`.
  - (browser) Logical properties hold. A paragraph's indent is on its inline start (top). Figures are bounded by `max-block-size`. Line numbers and note marks sit beside their block, not over the text: their boxes don't intersect the first line's box.
- [ ] **Step 2: Run to verify they fail.**
- [ ] **Step 3: Implement.** Convert `book.css` rules that use physical margins, padding or sizes for block flow to logical ones. In vertical, `--book-column-width` caps the line length, which is the text's `inline-size` (its height). Use `text-combine-upright: digits 2`.
- [ ] **Step 4: Run** the jsdom and browser files. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a Japanese edition set vertically, top to bottom and right to left".

### Task 5: `bookPages` learns the vertical axes

**Files:** Modify `web/src/bookPages.ts`. Test `web/tests/bookPages.browser.test.tsx` (extend) and `web/tests/bookPages.test.tsx`.

**Interfaces:**
- Consumes Task 4's measured axis facts.
- Produces:
  ```ts
  interface Flow { vertical: boolean; paged: boolean }
  function flowOf(text: HTMLElement): Flow   // from data-vertical, data-paged
  // Internal helpers used by every exported function:
  //   blockStart(rect) / blockEnd(rect) — edge of a box along the block axis, in "progress" units (grows as reading advances)
  //   progress(text) / setProgress(text, n) — scroll position along the reading axis, in non-negative units
  //   pageSize(text) — clientWidth or clientHeight on the page axis
  ```
  The exported signatures of `topBlock`, `topOf`, `shows`, `kept`, `show`, `pagesOf`, `turn`, `pageTurnedBy`, `movesAnchor` and `wordNodes` are unchanged.
- [ ] **Step 1: Write the failing browser tests**, each run in all four flows (horizontal or vertical × scroll or paged), with a 40-block fixture:
  - `show(text, 20)`, then `topBlock(text) === 20`, and `topOf(text)` gives `{block: 20, at: 0}`.
  - `show(text, 20, 37)`, then `shows(text, {block: 20, at: 37})`.
  - `turn(text, 1)` moves forward. `topBlock` grows, and in paged `pagesOf(...).at` grows by 1. `turn(text, -1)` goes back.
  - `pagesOf` gives `of` ≥ 2 for the fixture in both paged flows and `undefined` in both scroll flows.
  - Existing horizontal tests pass unchanged.
- [ ] **Step 2: Run to verify they fail** in the vertical flows.
- [ ] **Step 3: Implement.** Every read of `scrollLeft`, `scrollTop`, `clientWidth`, `clientHeight`, `rect.left`, `rect.right`, `rect.top` or `rect.bottom` goes through the helpers. Vertical scroll's negative `scrollLeft` is turned into non-negative progress in `progress`/`setProgress` only.
- [ ] **Step 4: Run** `vitest --project browser tests/bookPages.browser.test.tsx` and the jsdom `bookPages` tests. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: the reader's place, pages and turns measured along a vertical book's axes".

### Task 6: Vertical page turns, and the reader's place across switches

**Files:** Modify `web/src/bookPages.ts` (`pageTurnedBy`) and `web/src/BookReader.tsx` (key step mapping and re-anchoring effects). Test `web/tests/BookReader.test.tsx` and `web/tests/bookPages.browser.test.tsx`.

**Interfaces:** Consumes Task 5's `flowOf`.
- [ ] **Step 1: Write the failing tests:**
  - (browser) In vertical paged, `pageTurnedBy` at x on the left third gives +1 and on the right third gives −1. Horizontal is unchanged.
  - (jsdom) In vertical, ArrowLeft gives step +1 and ArrowRight gives step −1. Horizontal is unchanged. The step intents are untouched.
  - (browser, **Review Focus 3**) In scroll and in paged, place the reader at block 15, toggle vertical on, and `topBlock` is 15. Toggle it off and it is still 15. The same holds for swapping between a `ja` and an `en` edition with vertical on (counterpart block via the existing swap), and for switching scroll to interlinear and back.
  - (browser, **Review Focus 2**) In vertical paged, a search pick (`show` at a block mid-book plus a found range) leaves the `.book-found` element's box inside the visible page.
- [ ] **Step 2: Run to verify they fail.**
- [ ] **Step 3: Implement.** Re-anchor in a layout effect keyed on `[vertical, mode, paged, edition]`, taking `topOf` in the render before the change. Follow the pattern the resize handler uses.
- [ ] **Step 4: Run** the browser and jsdom files. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a vertical book's pages turn right to left, and its place holds through every switch".

### Task 7: Gate, status and real books

**Files:** Modify `docs/status.md`.

- [ ] **Step 1: Run the full gate.** `scripts/check-all.sh > target/check-all.log 2>&1; echo "exit $?"`, in the background and polled, with a pnpm shim on PATH (`npx pnpm@11.3.0`). Expected: `exit 0`, except for the known flaky `Filmstrip.browser.test.tsx`, which must be re-run alone and reported.
- [ ] **Step 2: Open real books** against a **copy** of the database in the scratchpad (never the user's own), with screenshots:
  - *Botchan* vertical, scroll and paged: furigana beside the text, a page turn by tap on the left, a footnote or figure if any.
  - Search for `坊っちゃん` and step through.
  - Search an English book for a name across all editions.
- [ ] **Step 3: Update `docs/status.md`.** Fill in the Books entry, add an O-line per gap a reader would notice (starting at the next free number), and correct the test counts.
- [ ] **Step 4: Commit:** "Books search and vertical Japanese: status".

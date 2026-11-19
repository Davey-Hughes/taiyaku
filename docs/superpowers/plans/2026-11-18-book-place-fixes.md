# Book Place Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A docked panel in compare paged keeps the reader's text on the page, and a search steps from the reader's place unless the match last gone to is still on screen.

**Architecture:** `stepFrom` uses the reader's place whenever the match given is not listed, including when there is none. `Anchor` gains an optional `read` character, taken on a page a compared row carries over onto and preferred by `show` over the row's fraction.

**Tech Stack:** TypeScript, React, Vitest (node, jsdom and Chromium browser projects).

**Spec:** `docs/superpowers/specs/2026-11-18-book-place-fixes-design.md`

## Global Constraints

- Work in `.claude/worktrees/book-place-fixes` on branch `book-place-fixes`. Its `web/node_modules` is a copy. Run vitest as `./node_modules/.bin/vitest` from `web/`, with `--reporter=default`, into a log file.
- `stepFrom` keeps its contract: a listed `from` is stepped on from (`bookSearch.test.ts`'s "A match listed is stepped on from" stays as it is). Task 3 decides what its callers pass.
- Single-column paged, scrolled, interlinear and vertical text place exactly as they do now, and so does a docked panel opened or closed in the phone layout. Compared and paged, an image loading keeps the character read on the page shown, in every layout.
- A code comment says why, never what the code used to do or how a defect was found.
- Take no O-number: open PRs hold the next ones. Add no dependency.
- Evidence for this branch goes in `.superpowers/sdd/book-place-fixes-evidence/`, never `target/`.

## Review Focus

1. A panel opened, closed and opened again on the same page: the second opening lands on the character's page too.
2. A compared page holding only the other column's words: placing falls back to the row's fraction, and closing restores the page.
3. Text size changed on a page a row carries over onto: the reader goes to the row's start, as now, not to the remembered character.
4. A first search from a place past the last match or before the first: Enter and Shift+Enter go round the ends.
5. The edition read swapped in compare with a panel open: the remembered character of the old edition is not used.

---

### Task 1: The first search starts from the reader's place

**Files:**
- Modify: `web/src/bookSearch.ts` (`stepFrom` and its doc comment)
- Modify: `web/src/BookSearch.tsx` (the `here` prop's comment), `web/src/BookReader.tsx` (the comment above `here`)
- Test: `web/tests/bookSearch.test.ts`, `web/tests/BookSearch.test.tsx`, `web/tests/bookPlace.browser.test.tsx`
- Modify: `docs/superpowers/specs/2026-11-16-book-search-and-vertical-design.md` (the sentence "The first search after opening still steps from the book's first match."), `docs/status.md` (the book search paragraph's "where the match last gone to is no longer listed" sentence)

**Interfaces:**
- Produces: `stepFrom(matches, from, by, here?)`, signature unchanged. With `here` given and `from` undefined or not listed, it steps from `here`. With no `here`, it returns the first or last match.

- [ ] **Step 1: Write the failing unit test** in `bookSearch.test.ts`'s `stepFrom` block, named `steps from the reader’s place where no match has been gone to`. With that block's `listed`, `m` and `here`:
  - `stepFrom(listed, undefined, 1, here(1, 4))` is `m(1, 8)`, and `-1` is `m(1, 0)`
  - `stepFrom(listed, undefined, 1, here(1, 0))` is `m(1, 0)`
  - `stepFrom(listed, undefined, 1, here(4, 0))` is `m(0, 5)`
  - `stepFrom(listed, undefined, -1, here(0, 0))` is `m(3, 2)`

- [ ] **Step 2: Write the failing jsdom test** in `BookSearch.test.tsx`. Give `server` an optional saved slot that the progress request answers with `{ positions: [{ edition: 'en', slot, updated: 'now' }] }`. Using `alternating`:
  - saved at 6, `ron`: Enter lights block 7 and the progress `PUT` carries slot 7
  - saved at 6, `ron`: Shift+Enter lights block 5
  - saved at 6, `harry`: Enter lights block 6
  - saved at 9, `harry`: Enter lights block 0
  - saved at 0, `ron`: Shift+Enter lights block 9

- [ ] **Step 3: Write the failing browser test** in `bookPlace.browser.test.tsx`, with the 40-block `marked` fixture from `.superpowers/sdd/book-place-fixes-evidence/bookPlace-probe-addition.tsx`. Opened at slot 12 in each of `FLOWS`, and compared and paged in English, the first Enter lights block 15 and the first Shift+Enter lights block 10.

- [ ] **Step 4: Run the three files and watch them fail.** Expected: the wrong blocks are 0 and 35 in the browser test, and 1 and 9 for `ron` in the jsdom test.

- [ ] **Step 5: Implement.** In `stepFrom`, the early return asks only `!here`. Reword the three comments and the two doc sentences to say the first search steps from the reader's place.

- [ ] **Step 6: Run the three files and watch them pass.**

- [ ] **Step 7: Watch the mutation.** Put `!from ||` back. All three new tests must go red. Restore it and record the result in the evidence directory.

- [ ] **Step 8: Commit.**

### Task 2: Compare paged keeps the character read through a panel

**Files:**
- Modify: `web/src/bookPages.ts` (`Anchor`, `anchorOf`, `whole`, `show`, one new private helper beside `rowWords`)
- Modify: `web/src/BookReader.tsx` (`Place`, the place effect that sets `anchor.current`)
- Test: `web/tests/bookPlace.browser.test.tsx`, `web/tests/bookPages.test.tsx`
- Modify: `docs/superpowers/specs/2026-11-16-book-search-and-vertical-design.md` (the sentence "Compared, the anchor is placed through the row holding it"), `docs/status.md` (the docked panel sentence of the book search paragraph)

**Interfaces:**
- Produces: `Anchor.read?: { block: number; at: number }`, the first character of the column read on the page shown. Set only compared, paged, on a page a row carries over onto. `whole` removes it.
- Consumes: `show(text, block, at, { figure, into, read })`.

`.superpowers/sdd/book-place-fixes-evidence/prototype.diff` is an unreviewed starting point. Share its character lookup with `rowWords` rather than repeating the selector.

- [ ] **Step 1: Write the failing sweep** in `bookPlace.browser.test.tsx`, adapted from `zzproof.browser.test.tsx.txt` in the evidence directory, named `keeps the first character of the column read on the page through a panel, compared and paged`. Windows: 1000×700, 1366×768, and 1920×1080 at text size 30. Editions: `en` and `ja`. On each carried page, hold a `Range` on the column's first on-page character, then:
  - open: the page shown is the character's page
  - close: `scrollLeft` is exact
  - open and close again: the same two assertions
  - the blind-fixture guard: for `en`, count samples where `floor(into × row pages)` names a different page from the character, and require more than 0

- [ ] **Step 2: Write the Review Focus tests.**
  - `bookPlace.browser`: a row whose left column is short and whose right column runs on, on a page holding only the right column's words. `anchorOf(text).read` is undefined, and closing the panel restores `scrollLeft`.
  - `bookPlace.browser`: the text size changed on a carried page puts the row's start on the page.
  - `bookPlace.browser`: the edition read swapped with a panel open, then closed, shows the counterpart block's row.
  - `bookPages.test.tsx`: `whole({ block: 3, at: 0, into: 0.5, read: { block: 3, at: 40 } })` equals `{ block: 3, at: 0 }`.

- [ ] **Step 3: Run them and watch the sweep fail** on the page assertions.

- [ ] **Step 4: Implement** `Anchor.read`, its capture in `anchorOf`, its removal in `whole`, its use in `show`, and carry it through `Place` and the place effect.

- [ ] **Step 5: Run `bookPlace`, `bookPages`, `bookVertical`, `bookOverlays`, `bookTaps` and `bookStyles` browser files and the jsdom book files, and watch them pass.**

- [ ] **Step 6: Watch the mutations.** Each must turn a named test red: drop `read` from the place effect, keep `read` in `whole`, ignore `read` in `show`, and take `read` from the other column. A mutation nothing catches is reported, not forced. Record the results in the evidence directory.

- [ ] **Step 7: Update the two docs. Commit.**

### Task 3: A step goes from the place once the reader has moved away from the match

**Files:**
- Modify: `web/src/bookPages.ts` (a new exported `matchShows`, sharing `shows`'s on-screen test)
- Modify: `web/src/BookReader.tsx` (a new `steppedFrom` callback beside `here`, used by the `SEARCH_NEXT` / `SEARCH_PREVIOUS` keys and passed to `BookSearch`)
- Modify: `web/src/BookSearch.tsx` (the `found` prop becomes `from: () => Match | undefined`, asked at each step as `here` is)
- Test: `web/tests/bookPlace.browser.test.tsx`, `web/tests/BookSearch.test.tsx`
- Modify: `docs/superpowers/specs/2026-11-16-book-search-and-vertical-design.md` (the "step to the match after or before the one last gone to" line), `docs/status.md` (the book search paragraph's stepping sentence, and the web test counts on the "Six crates" line, measured from the `node` and `jsdom` projects)

**Interfaces:**
- Consumes: `stepFrom(matches, from, by, here)` from Task 1, unchanged. With `from` undefined it steps from `here`.
- Produces: `matchShows(text: HTMLElement | null, match: { block: number; start: number }): boolean`, whether the match's first character is on the page or screen showing. It must answer for a compared row too, by the column read, where `shows` answers false.
- Produces: `steppedFrom(): Match | undefined`, the match last gone to when all of these hold, otherwise undefined:
  - its edition is the edition read
  - the anchor is still at it (`block` and `at` equal the match's `block` and `start`), or `matchShows` answers true

The anchor check comes first because a match just gone to must always be stepped on from, even where the layout gives no box to measure. Without it a step could land on the same match again and never move on.

- [ ] **Step 1: Write the failing browser tests** in `bookPlace.browser.test.tsx` with Task 1's `marked` fixture (a mark in every fifth block), in each of `FLOWS` and compared and paged in English, opened at slot 12, the first Enter lighting block 15:
  - read on until block 15 is off screen and the top block is at least 22: `]` lights block 25 (the old code lights 20)
  - the same, with `[`: lights block 20 (the old code lights 10)
  - the same, with the search box reopened and Enter: lights block 25
  - moved so block 15 is still on screen and the place is before it (scrolled back a little, or a page turned on and back): `]` lights block 20, not 15 again

- [ ] **Step 2: Write the Review Focus tests.**
  - read on past the last match (block 35): `]` lights block 0, and `[` lights block 35
  - moved away by the contents panel rather than by scrolling: `]` steps from the new place
  - **All editions**, the edition read swapped by hand after a match: `]` steps from the place in the edition now read

- [ ] **Step 3: Run them and watch them fail** for the reasons named.

- [ ] **Step 4: Implement** `matchShows`, `steppedFrom` and the `from` prop. Reword the comments on `here`, `found` and `stepFrom`'s callers to state the rule.

- [ ] **Step 5: Run the book browser files and the jsdom book files, and watch them pass.** The existing `]` tests in `BookSearch.test.tsx` stay green unchanged.

- [ ] **Step 6: Watch the mutations.** Each must turn a named test red: `steppedFrom` always returns the match; it never returns the match; the anchor check removed; the edition check removed; `matchShows` answering as `shows` does in compare. Record the results in the evidence directory.

- [ ] **Step 7: Update the two docs, measure the web test counts from the whole `node` and `jsdom` projects, and commit.**

### Task 4: A window resize as a book opens keeps the saved place

**Files:**
- Modify: `web/src/BookReader.tsx` (the window `resize` effect's `keep`, beside `placedBefore`)
- Test: `web/tests/bookPlace.browser.test.tsx`
- Modify: `docs/status.md` (one sentence in the book reader's notes)

**Interfaces:**
- Consumes: `placedBefore`, the place last committed.

The mechanism and a proving test are in `first-open-report.md` in the evidence directory.

- [ ] **Step 1: Write the failing test**, named `a window resize as the book opens keeps the saved place`, single column paged, single column scrolled, and compared and paged. Hold the book's fetch. Inside one `act`, release it, wait for its answer and dispatch `resize`. The top block is 12, and no progress `PUT` carries slot 0.

- [ ] **Step 2: Run it and watch it fail** with block 0.

- [ ] **Step 3: Implement.** A place not yet committed is left alone: `keep` replaces the place only when it is `placedBefore.current`, so the commit places the saved block at the new size.

- [ ] **Step 4: Check the other re-places.** The image-load and panel re-places also read `anchor.current`. Establish by a test or by reading whether either can replace a place not yet committed. Guard and test any that can, and say in the report why any cannot.

- [ ] **Step 5: Watch the mutation.** Remove the guard, and the new test goes red. Restore it and record the result in the evidence directory.

- [ ] **Step 6: Update the sentence and commit.**

### Task 5: The panel keeps the reader's text on a book just opened and on a row opening with a figure

**Files:**
- Modify: `web/src/bookPages.ts` (`anchorOf`, `whole`, `show`, and what they share with `readOnPage`)
- Modify: `web/src/BookReader.tsx` (the place effect, and the first placing of a saved block)
- Test: `web/tests/bookPlace.browser.test.tsx`, `web/tests/bookPages.test.tsx`
- Modify: `docs/status.md` (the docked panel sentence and the "Six crates" counts), `docs/superpowers/specs/2026-11-16-book-search-and-vertical-design.md` (the re-placing sentence)

**Interfaces:**
- Produces: `Anchor.read`, now set compared and paged on any page holding a character of the column read, taken after a scroll and after a placing, except a placing at a search match. `whole` still removes it, including when `into` is undefined.

The probes that reproduce both cases are in `panel-probes/` in the evidence directory, and `rebase-and-compare.md` holds their figures. They are throwaway: take the fixtures and write the tests in the file's own style.

- [ ] **Step 1: Write the failing sweeps** in `bookPlace.browser.test.tsx`, at 1000×700 and 1366×768:
  - **Just opened:** a book saved at each of several blocks, including a block inside a merged row and a row's own block whose row opens with a figure. The page shown holds the saved block's first character in the column read. Open a panel: the page shown holds it. Close it: `scrollLeft` is the page the book opened on. Repeat once.
  - **A row opening with a figure:** with a real 300×300 image and the shipped stylesheet, and with the forced 300×300 fixture, on every page turned to that holds a character of the column read: open a panel and the page shown holds that page's first such character; close it and `scrollLeft` is exact. Repeat once. Require at least one sampled page whose row opens with a figure.

- [ ] **Step 2: Write the tests that pin what must not change.**
  - Gone to through a search match later on a page, a panel keeps the match on the page shown.
  - A page holding only the other column's words falls back to the row's fraction, and closing restores the page.
  - A text-size change on a carried page still goes back to the row's start.

- [ ] **Step 3: Run them and watch the sweeps fail** on the page assertions.

- [ ] **Step 4: Implement.** Take `read` on every compared paged page holding a character of the column read, and after a placing that is not at a match. The first placing of a saved block places by that block's own words, as a later re-place does, so opening, a panel and closing agree on one page.

- [ ] **Step 5: Run every book browser file and the jsdom book files, and watch them pass.**

- [ ] **Step 6: Watch the mutations.** Each must turn a named test red: `read` taken only on a carried page; `read` taken after a match's placing; the first placing back on the row's start; `whole` keeping `read` when `into` is undefined. Record the results in the evidence directory.

- [ ] **Step 7: Keep the coverage floor.** Add jsdom tests in `bookPages.test.tsx` for the new paths, in that file's style, so `scripts/coverage-floor.mjs` passes. Lower no floor.

- [ ] **Step 8: Update the docs so each sentence is true of the code, measure the web test counts from the whole `node`, `jsdom` and `browser` projects, and commit.**

### Task 6: Reopening a paged book returns to the page it was left on

**Files:**
- Modify: `web/src/bookPages.ts` (a new exported `resumeBlock`)
- Modify: `web/src/BookReader.tsx` (the `save` in the text's scroll handler)
- Test: `web/tests/bookPlace.browser.test.tsx`, `web/tests/bookPages.test.tsx`
- Modify: `docs/status.md` (one sentence in the book reader's notes, and the "Six crates" counts)

**Interfaces:**
- Produces: `resumeBlock(text: HTMLElement | null): number`. Paged, the first block whose first line is on the page showing, or `topBlock` where none is. Scrolled, `topBlock`.

`reopen-report.md` in the evidence directory holds the mechanism and the measurements. `reopen/` beside it holds the probe, the ten-case test and an unreviewed candidate fix, written before Tasks 4 and 5 landed.

- [ ] **Step 1: Write the failing test** in `bookPlace.browser.test.tsx`, adapted from `reopen/reopenPlace.browser.test.tsx`, named `reopening a paged book returns to the page it was left on`. Each open feeds the progress `PUT` the last one sent to the next. Single column paged, compared paged, and vertical paged, at 1000×700 and 390×844. Each case first asserts the page it needs: one starting inside a carried-over block, and one no block starts on. Across at least five reopens the page and the first character shown stay the same, both after opening and closing with no reading and after turning pages and leaving.

- [ ] **Step 2: Add the cases this branch makes specific.**
  - Compared, saved at a block inside a merged row: repeated opens keep that block's first words on the page shown, and the saved block does not walk back.
  - A page no block starts on: the first reopen lands on the first page of the block carried onto it, and later reopens hold there.
  - Scrolled, single column and compared: the saved block is unchanged, as a control.

- [ ] **Step 3: Run them and watch them fail** on the walk back.

- [ ] **Step 4: Implement** `resumeBlock` and save it from the scroll handler. A first search and a step read the anchor, not the saved block, so they do not change.

- [ ] **Step 5: Run every book browser file and the jsdom book files, and watch them pass.** That includes Task 4's save assertion and Task 5's opening sweeps.

- [ ] **Step 6: Watch the mutations.** Each must turn a named test red: the handler saves `topBlock` again; `resumeBlock` ignores whether a block's first line is on the page; `resumeBlock` answers a compared row by its first block instead of the block saved inside it. Record the results in the evidence directory.

- [ ] **Step 7: Keep the coverage floor.** Add jsdom tests in `bookPages.test.tsx` for `resumeBlock`, so `scripts/coverage-floor.mjs` passes. Lower no floor.

- [ ] **Step 8: Update the sentence, measure the web test counts from the whole `node`, `jsdom` and `browser` projects, and commit.**

## After the tasks

Whole-branch review, then `scripts/check-all.sh` and the whole browser project, each into a log with its exit status read from the log, then the claim check over the docs and PR body.

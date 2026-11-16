# The Continue row across a resize, and covers that never spill (O301, O302) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A window resize keeps the Continue row's place as a cover-size change does, a change in the number of cards drops a kept place, the place's two untested parts get tests, and below 900 px no cover is wider than its list.

**Architecture:** `useContinueRow` (`web/src/continueRow.ts`) keeps a `Place` in a ref and, on a cover-size change, puts the row back from it. Pull that put-back into one step inside the effect, call it from the `ResizeObserver` callback too, and record the card count in the place so a change in it re-marks. The overflow is one CSS rule in `web/src/styles/library.css`'s `@media (width < 900px)` block.

**Tech Stack:** React 19, TypeScript, Vitest 4 (projects `node`, `jsdom`, `browser`, `browser (chromium, scrollbars shown)`), Biome.

**Spec:** `docs/superpowers/specs/2026-11-15-continue-row-resize-design.md`

## Global Constraints

- The row stays laid out from outside React and before paint. A resize or a scroll renders nothing, as `switches layout across 900 px, … rendering nothing` already asserts.
- The kept place is never re-read from `scrollLeft` during a resize. It reads back rounded or clamped and would drift over a drag's steps.
- Every new test is watched failing against the defect it names, by hand-applying that defect to the code, and the defect is restored afterwards. A test that passes on `main` (the `end > 1` one, the scaled offset one) counts only once its mutation has turned it red.
- A code comment says why (the reason, the invariant, what breaks); no history, no measurements (`scripts/check-comments.mjs` gates this).
- Run vitest as `./node_modules/.bin/vitest` from `web/`, never `pnpm exec` (worktree node_modules). Set `--reporter=default` when reading console output.
- No `sizes="auto"`; no change to the cover-size path's behaviour.

## Review Focus

1. **A window drag**: many resize reports in a row (a frame a step). The card at the start edge must not creep over 20 steps. Test: Task 1, step 5 (B2).
2. **A row scrolled to its end that fits at a wider window and overflows again when narrowed**: it must be at its end again, and a row seen fitting must never count as at its end. Test: Task 1, step 5 (B3); hook test H4.
3. **The user scrolls between two resizes**: the second resize must keep the new place, not the one before the scroll. Test: Task 1, hook test H6.
4. **The sidebar turning at 900 px during a resize sequence**: the list's width changes by the sidebar's column, not only the window's. Test: Task 1, B1's widths cross 900.
5. **A cover-size change right after a resize** (both paths share the put-back): the place the resize kept is the one the size change uses. Test: Task 1, hook test H7.

---

### Setup (controller, before Task 1)

- In the worktree `/home/davey/projects/taiyaku/.claude/worktrees/continue-row-resize`: `cp -a` (never symlink) `web/node_modules` and `crates/taiyaku-wasm/pkg` from `/home/davey/projects/taiyaku`.
- Check: `cd web && ./node_modules/.bin/vitest run --project jsdom tests/continueRow.hook.test.tsx` passes on the branch head.
- Another session (first-scan) runs on this machine. Read `/proc/loadavg` before believing a red browser test that has no failed assertion (see the browser-gate memory).

### Task 1: The row keeps its place across a resize, and drops it when the count changes (O301)

**Files:**
- Modify: `web/src/continueRow.ts` (`Place`, `useContinueRow`: `mark`, `resized`, the size-change branch, the hook's doc comment)
- Test: `web/tests/continueRow.hook.test.tsx` (jsdom), `web/tests/Library.continue.browser.test.tsx` (browser)

**Interfaces:**
- Consumes: nothing new.
- Produces: `useContinueRow(list, drawn, coverSize, count): void`, unchanged signature. `Place` gains `count: number`.

- [ ] **Step 1: Hook tests (jsdom), in `describe('useContinueRow')`, written first.** Extend the file's geometry: give `scrollLeft` a setter that clamps to `[0, scrollWidth - clientWidth]` as a browser does, and give each `li` a `getBoundingClientRect().left` of `index * step` from a mutable `step`, so a change in `step` is a change in pitch. Each test marks the place by setting `scroll.left`, dispatching `scroll` and running `frame()`. The values below are chosen so that right and wrong give different numbers.
  - **H1** `keeps the card at its start edge across a resize, the same fraction past its edge`: 12 cards, step 160, scrollWidth 1920, client 1000, left 400 (2.5 cards). Then step 200, scrollWidth 2400, client 1200, `resized()`. Expect `scrollLeft` close to 500. The defect it names: `resized` re-marks without putting the row back, leaving 400.
  - **H2** `keeps its end across a resize`: as H1 but left 920 (the end). After the resize, expect `scrollLeft` = 1200 (the new end). With the defect: 920.
  - **H3** `scales the offset into the start card to the new pitch on a size change`: step 160, left 400. Rerender `size={164}` with step 180. Expect 450. The defect: the offset kept in px, `2 * 180 + 80` = 440.
  - **H4** `never counts a row that fits as at its end`: 6 cards, step 160, scrollWidth = client = 1000, left 0, mark. Rerender `size={200}` with step 200, scrollWidth 1200. Expect 0. The defect: `atEnd` without `end > 1` sends it to 200.
  - **H5** `drops a kept place when the number of cards changes` (O301's sequence): 12 cards, step 160, scrollWidth 1920, client 1000, left 920 (the end), mark. Rerender `size={96}` with step 80, scrollWidth 960 (fits, clamps to 0). Rerender `count={13}` at that size, scrollWidth 1040 (overflows, left still 0). Rerender `size={144}` with step 160, scrollWidth 2080. Expect 0. The defect: the place outlives the count and sends the row to 1080.
  - **H6** `keeps a place scrolled to between two resizes`: H1's first resize, then left 600 (3 cards at 200), mark, then step 160 / scrollWidth 1920 / client 1000, `resized()`. Expect 480. The defect: the second resize restores from the place before the scroll (2.5 cards, 400).
  - **H7** `keeps a resize's place through a cover-size change after it`: H1's resize, then rerender `size={164}` with step 220. Expect 550 (2.5 × 220).
- [ ] **Step 2: Run** `./node_modules/.bin/vitest run --project jsdom tests/continueRow.hook.test.tsx`. Expected: H1, H2, H5, H6 and H7 fail. H7 fails because its resize leaves 400, so the size change puts 2 cards at 220, which is 440. H3 and H4 pass on `main`, and Step 4's mutations are what turn them red.
- [ ] **Step 3: Implement in `web/src/continueRow.ts`.**
  - `Place` gets `count`. `mark` keeps the place only while `ul`, `left`, `step` and `count` are unchanged, and records `count` when it marks.
  - One put-back step inside the effect (for example `restore(was: Place): void`) sets `scrollLeft` from `was.atEnd` or `was.first * stepOf()`, then records the place as now laid: the size, the new `left` and `step`, and the count.
  - The size-change branch calls it. `resized` takes `place.current` before `measure()` and, when it is this list's, calls it after `measure()` and before `mark()`.
  - The hook's doc comment says a resize keeps the place too, and says why a count change drops it: a card more or fewer moves no `scrollLeft` and no pitch, so nothing else would re-mark.
- [ ] **Step 4: Run** Step 2's command. Expected: all pass. Then hand-apply each defect named in H1–H6 (no put-back in `resized`; no count in the keep check; no `end > 1`; an unscaled offset) one at a time, run, see the named test fail, and restore. Record each red run's failing test name in the task report.
- [ ] **Step 5: Browser tests**, in `describe('the Continue row over the grid (O288 item 2)')`, shaped like the existing `keeps %s across cover-size changes…` test (its `off`, `stepped`, `near` and `seen` helpers):
  - **B1** `it.each(['its end', 'the card at its start edge'])` `keeps %s across window resizes, a phone turning and back, also across 900 px`: `drawn({ width: 390, size: 144, entries: 12 })`, scroll to the third card's edge or to the end, `frames(2)`, then `page.viewport(w, 844)` for w in 844, 1280, 899 and 390. After each, poll `stepped` and record `{ columns, off: near(off(), 0) }`. Assert the whole `seen` list, with each width's column count read off the fixture once and written in, so that "the fixture tells" each width changes the columns.
  - **B2** `does not creep over a window drag`: as B1's start-edge case, then 20 viewport steps from 390 to 844, one `frames(1)` each. Expect `near(off(), 0)` = 0 at the end.
  - **B3** `keeps its end through a window at which every card fits`: entries and widths chosen so the row overflows, fits at a wider window, and overflows again when narrowed. Scrolled to its end first, it is at its end again. The fixture asserts `fits` and `overflows` as the existing size version does.
- [ ] **Step 6: Run** `./node_modules/.bin/vitest run --project browser tests/Library.continue.browser.test.tsx`. Expected: all pass. Hand-apply "no put-back in `resized`", and B1 and B2 fail. Restore it.
- [ ] **Step 7:** `./node_modules/.bin/biome check ./src ./tests` and `./node_modules/.bin/tsc -b --noEmit` (or `pnpm -C web run typecheck` from the worktree root) are clean. Commit: `The Continue row keeps its place across a window resize, and a change in the number of cards drops a kept place (O301)`.

### Task 2: Below 900 px a cover is never wider than its list (O302)

**Files:**
- Modify: `web/src/styles/library.css` (the `@media (width < 900px)` block's `.grid, .volumes` rule and its comment; the `.library` tokens comment where it says the slider sets the least a cover may be)
- Test: `web/tests/coverLayout.browser.test.tsx` (`describe('a cover list below 900 px (O285)')`), `web/tests/Library.continue.browser.test.tsx` (the `lays its cards over the grid's columns…` table)

**Interfaces:** none.

- [ ] **Step 1: Tests first.**
  - In `coverLayout.browser.test.tsx`: `it.each([HOME, SERIES])` `never lays a cover wider than the list on %s, at a 260 px window and 240 px covers`. Draw it, then expect one column, the list not to overflow (`scrollWidth <= clientWidth`), and each tile's width within 0.5 px of the list's content width. The home list at 260 px is 228 px (the window less 32 px), so a 240 px column overflows on `main`. If a series page's list differs, the test reads it from the page and does not assume it.
  - In `Library.continue.browser.test.tsx`: add `[260, 240, 3]` to the table. On `main` the grid's column is 240 px while the row's lone card is the list's width, so `offGrid` is not empty.
- [ ] **Step 2: Run** both files in the browser project. Expected: the new cases fail.
- [ ] **Step 3: Implement**: `grid-template-columns: repeat(auto-fill, minmax(min(var(--tile-min), 100%), 1fr));`. The comment says why: where the list is narrower than one cover, the slider's least size cannot hold, so the column takes the list's width. The tokens comment's "the slider sets the least a cover may be" gains that exception. `library.ts`'s `coverSizes` is unchanged: below about 272 px it names a size larger than drawn, which costs a larger image and nothing else.
- [ ] **Step 4: Run** Step 2's files, and expect a pass. Hand-revert the CSS, see both new cases fail, and restore it.
- [ ] **Step 5:** Biome clean. Commit: `Below 900 px a cover is never wider than its list (O302)`.

### Task 3: The documents

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`

- [ ] **Step 1: O301 closes (the five-part edit, from the open-items ritual).** The row: `[x]`, and a closing statement of what shipped in the row's own style. The bullet below the table is rewritten to say what was done. In O288's row, "A window resize does not keep it (O301)." becomes what is true now. In the shipped-work bullet (the one ending "…O301, below, is open for it."), the same. Then the counts: the closed count, "leaving N", and the "What remains" line (its count word wraps across a line break, so grep the sentence opener and read the next line).
- [ ] **Step 2: O302 opens and closes** in this PR: a table row after O301 in the same style (closed, naming `library.css`'s below-900 px rule by symbol, never a line number) and a bullet. The total count word at the head of the section moves by one.
- [ ] **Step 3: `docs/notes.md`, "O285 and O288"**: the paragraph "A question for the owner at the PR, not decided." becomes the ruling: the owner approved `minmax(min(var(--tile-min), 100%), 1fr)`, and it shipped as O302. Add a short section, `## O301 and O302: the Continue row across a resize, and covers that never spill (2026-11-16)`, giving the tests and the defect each was watched failing against. Keep figures to the ones the tests pin.
- [ ] **Step 4: Test counts.** Run `./node_modules/.bin/vitest run --project node` and `--project jsdom` from `web/`, and write the counts they report into `status.md`'s web test clause. Never add to the number already there. The Rust count is unchanged.
- [ ] **Step 5: Sweep.** Run the four greps from the open-items ritual (rows, closed, test counts, "What remains" with `-A1`). Grep the bare `O301` and `O302` across `docs/` and `web/src/`, read every hit, and fix any present-tense "open" or "does not keep".
- [ ] **Step 6:** Commit: `Close O301 and O302 in the status and the notes`.

### Finish

1. **Whole-branch review** of `main..HEAD`, against the spec and the Review Focus. It reviews any fix rounds too. After a fix, grep for the finding's shape, not only its function.
2. **Gate**: `scripts/check-all.sh` from the worktree root, on the final commit, into a log file, never piped through `tail`. Quote its `all green` line. The browser projects run only here, not in CI.
3. **Claim check** of the PR body, the status rows and the notes before saying "ready to merge", and again after any correction.
4. **PR** on Gitea (the MCP, not `gh`). Title under 255 characters and measured, in the house style (what the user sees, then the O-numbers). The body is not hard-wrapped and carries no attribution.

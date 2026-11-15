# Compact Series Tiles, After the Drive: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A title save out for 200 ms says "Saving…", the Escape that puts back the stored title also takes the failure away, and a last-row panel ends the page with as much room below it as the last row has (O283, PR #248).

**Architecture:** Two small changes on branch `compact-tiles` before #248 merges. `MetadataEditor` gains a state for "a title save is out", drawn through the existing `useDrawnWait` (`web/src/hooks.ts`) in the "Saved" note's place, and its Escape clears `failure`. The panel gains an unpainted box below it, as tall as the page area's block-end padding (plus the bottom safe-area inset below 900 px), so the page's scroll end lies that far below the panel's foot.

**Tech Stack:** React 19, TypeScript, CSS, vitest (`node`, `jsdom`, `browser` projects).

**Spec:** `docs/superpowers/specs/2026-11-14-compact-series-tiles-design.md`, "Settled after the drive", items 9-11 (2350c1e0). The rest of that spec stays in force.

## Global Constraints

- The O283 plan's Global Constraints apply unchanged (`docs/superpowers/plans/2026-11-14-compact-series-tiles.md`, "## Global Constraints"): comments, attribution, web checks per task, gate line, browser-case rules, watched mutations, pre-existing failures, classes, commits (branch `compact-tiles`, main checkout, never `--amend`), no Rust.
- **Copy, exact:** `Saving…` (one U+2026 ellipsis, as `LibraryPage.tsx`'s `SAVING`), `Saved`.
- **Wait threshold:** `WAIT_DRAWN_AFTER_MS` (200) through `useDrawnWait`; never a second timer or a copied `200`.
- **Room under the panel:** the page area's block-end padding, `1rem` at 900 px and wider and `var(--space-4)` below (`shell.css`, `.library > main`). Named once as a custom property set where that padding is set and read by both; never a second copy of either length. Below 900 px, where the document scrolls, the box's height also adds `env(safe-area-inset-bottom, 0px)`, as `.library`'s bottom padding (`shell.css` ~377-381) adds it under the last row there, so a phone that reports an inset gets the same room under the panel (spec item 11). Headless Chromium reports 0; no device check is planned.
- `E=/home/davey/projects/taiyaku/.superpowers/sdd/compact-tiles-evidence`. Reports: `$E/D<n>-report.md` (D for "after the drive"), written by Bash heredoc (the Write tool is refused under `.superpowers/`).

## Review Focus

1. A save chained after the one out (a title typed while a save was out): "Saving…" must not give way to "Saved" between the two. Task 1 (c).
2. A save that fails after "Saving…" showed: the note must go with the failure's arrival, not stay saying "Saving…" beside an alert. Task 1 (d).
3. The unwritten note ("Saved — but this correction could not be saved beside your library") through the Escape that clears a failure: it speaks of the stored title and stays. Task 1 (f).
4. A press in the new room below a panel must land on what is under it, never on the panel. What is under it is the scroller (`main` at 1280, the document's root at 390), and a press on the scroller closes nothing (`Series.tsx`'s press listener; `docs/status.md`'s O283 bullet). Task 2 (i).
5. A panel laid back from its tile's right edge, or at 390 px where it is the area's width: the room box must not reach past the panel sideways and scroll the page sideways. Task 2 (j).

---

### Task 1: "Saving…" for a slow title save, and Escape takes a failure away

**Files:**
- Modify: `web/src/MetadataEditor.tsx` — `send()` (~576-617), the title field's key handler's Escape branch (~704-709), the `saved-note` paragraph (~1020-1022); the `saved` state's doc comment (~355) if it becomes false, and the `again` doc comment (~349-351), whose "which stays shown" Escape now ends.
- Modify: `web/src/styles/library.css` — only the comment above `.metadata-editor .saved-note` (~594-595), to name "Saving…" too; no rule changes.
- Test: `web/tests/MetadataEditor.test.tsx` (jsdom; fake timers as at ~599-617, the failed-save shape at ~636), `web/tests/MetadataEditor.browser.test.tsx` (beside ~491).

**Interfaces:**
- Consumes: `useDrawnWait(waiting: boolean): boolean` from `web/src/hooks.ts`.
- Produces: nothing other tasks use.

- [ ] **Step 1: Write the failing jsdom cases** in `MetadataEditor.test.tsx`, in the describe at ~526: (a)-(d) in a nested describe of their own with the fake-timer `beforeEach`/`afterEach` of ~597-603, settling by `drain` (`allLanded` waits on a `setTimeout` the fakes hold); (e) and (f) next to the failed-save Escape case. The status node is `status(host)[0]` (the describe's helper, ~531); a deferred `onPatch` is a promise the test resolves or rejects by hand.
  - (a) `it('says Saving… once a title save has been out 200 ms, and Saved as it lands')`: leave with a changed title; after `advanceTimersByTime(199)` the status text is `''`; after 1 more it is `'Saving…'`; resolve and drain: `'Saved'`. The same `status` node throughout (`toBe` the one read before the leave).
  - (b) `it('says no Saving… for a save that lands within 200 ms')`: at 150 ms, before the resolve, the status is `''`; resolve, drain: `'Saved'`; advance 100 ms more: still `'Saved'`.
  - (c) `it('keeps Saving… through the newer title sent after the save that was out')`: first save deferred; at 250 ms `'Saving…'`; type a second title and leave (queued, `patch` called once); resolve the first and drain: `patch` called twice and the status still `'Saving…'`; resolve the second: `'Saved'`.
  - (d) `it('takes Saving… away when the save fails')`: at 250 ms `'Saving…'`; reject with `new Error('offline')` and drain: status `''`, `[role="alert"]` text `'offline'`.
  - (e) `it('takes the failure away on the Escape that puts back the stored title')`: as the "puts back the stored title on Escape after a failed save" case; after Escape, the field is `'old'` and `host.querySelector('[role="alert"]')` is `null`. Assert the alert is there before the Escape.
  - (f) `it('keeps the note that a save was not written beside the library through that Escape')`: a first save answers not persisted (shape as the existing unwritten cases), a second fails; Escape: the alert is gone, `.unwritten` is still there, and the field holds the first save's title.
- [ ] **Step 2: Run them; (a) and (c)-(f) fail** on their assertions (`--project jsdom -t` per case, then the file). (b) may pass before the change, since nothing says `Saving…` yet: record that; it is watched in Step 5.
- [ ] **Step 3: Implement.** A boolean state, true from `send()`'s start until the chain it starts ends (both the success path that sets `sending.current = null` and the failure path), and never false between a save and the resend it chains. `const slow = useDrawnWait(thatState)`; the note reads `slow ? 'Saving…' : saved ? 'Saved' : ''`. The Escape branch adds `setFailure(undefined)` and leaves `unwritten` alone.
- [ ] **Step 4: Write the browser case** (g) beside ~491 in `MetadataEditor.browser.test.tsx`: `it('draws Saving… on the label’s line, so nothing moves as it comes and goes')`, at the describe's `WIDTH` (18rem). The fixture tells: `Saving…`'s drawn width is greater than `Saved`'s. With a deferred save, read the label's, field's and form's rects before the leave, after waiting 250 ms (`Saving…` shown), and after it lands (`Saved` shown): all three `toJSON()` equal across the three reads; the note's rect lies inside the label's first line box and right of the label's text, measured as the text's own width (`oneLineIn`), not the span's box, which spans the label.
- [ ] **Step 5: Watch each mutation fail, restore, `git diff` clean:** (1) `useDrawnWait(x)` → `x`: (a) and (b) red; (2) the note never says `Saving…`: (a) red; (3) the state set true in `saveTitle` rather than `send()`, and false at each landing, so the chained send leaves it false: (c) red (a false-then-true inside one `.then` is batched and never drawn, so a "false before the chained send" mutation cannot go red); (4) not set false on failure: (d) red; (5) not set false when the save lands: (b) red at +100 ms; (6) Escape without `setFailure(undefined)`: (e) red; (7) Escape also clears `unwritten`: (f) red; (8) `Saving…` drawn in a new in-flow paragraph under the label: (g) red.
- [ ] **Step 6: Whole projects:** node, jsdom, browser (CSS comment and a browser file changed), `tsc -b`, biome, coverage and the floor, per the Global Constraints. Quote each summary line.
- [ ] **Step 7: Commit** `web/src/MetadataEditor.tsx`, `web/src/styles/library.css`, the two test files: "A title save out for 200 ms says Saving…, and the Escape that puts back the stored title takes the failure away (O283)". Report `$E/D1-report.md`: commit, counts per project, each mutation with the case that went red.

### Task 2: Room below a last-row panel

**Files:**
- Modify: `web/src/styles/shell.css` — `.library > main` (~335-339) and its `(width < 900px)` rule (~572-575): the block-end padding through one custom property.
- Modify: `web/src/styles/library.css` — `.tile-panel` (~471): a `::after` box, directly after the `.tile-panel` rule (before `.volumes > li > .tile-panel`, for biome's descending-specificity rule), and its `(width < 900px)` height.
- Test: `web/tests/Series.scrollbars.browser.test.tsx`, in `describe('a volume’s panel run past the end of the page (O283)')` (~187).
- Test (node): new `web/tests/styles/library.css.test.ts`, reading the `?raw` source as `web/tests/styles/settings.css.test.ts` does; `atRulesOf` (`settings.css.test.ts` ~6-53) moves into `web/tests/support/css.ts`, exported, imported by both, and the new file joins that module's list of callers.

**Interfaces:**
- Consumes: Task 1's commit as base (no shared code).
- Produces: nothing.

- [ ] **Step 1: Write the failing browser cases** (with `onTestFinished(pointerRulesAs('mouse'))`; the scroller is `main` at 1280 and the document at 390, where `main` does not scroll):
  - (h) `it.each([1280, 390])('leaves as much room under an open last-row panel, scrolled to the end, as under the last row with no panel, at %i px')`. The fixture tells: with no panel the scroller scrolls, and the open panel's foot lies past the last tile's foot by more than the room. Scrolled to the end with no panel, `room0` = the shown area's bottom − the last tile's bottom; open the last tile's panel, scroll to the end, `room1` = the shown area's bottom − the panel's bottom. `room0 > 0`, and `room0` and `room1` each lie within 1 px of `main`'s computed `padding-block-end`. The tolerance is 1 px because the scroll end is a whole pixel, so each room carries its own rounding: 15.59 against 16.19 was measured on correct CSS.
  - (i) `it('lets a press in the room under the panel reach what is under it')`: scrolled to the end with a last-row panel open, `document.elementFromPoint` at the panel's left + 8 and its bottom + half the room is not inside the panel (assert first that the point lies inside the shown area).
  - (j) `it('scrolls nothing sideways for a panel laid back from its tile’s right edge, with the room under it, at 390 px')`: a right-hand tile in the last row at 390, panel open, scrolled to the end. The fixture tells: the tile is in the right-hand column, and its panel's left lies left of the tile's. Then the document's `scrollWidth` equals its `clientWidth`, and so does `main`'s.
  - (k) in `web/tests/styles/library.css.test.ts`: `.tile-panel::after`'s `height` at the top level names no `env(`. Under `@media (width < 900px)`, its `height` adds `env(safe-area-inset-bottom, 0px)` to the custom property that `main`'s `padding-block-end` reads in `shell.css`.
- [ ] **Step 2: Run them; (h) fails** at both widths on `room1` (the foot sits on the end), and (k) fails (no such rule). (i) and (j) may pass before the box exists: record that, and they are watched in Step 4.
- [ ] **Step 3: Implement.** `.library > main` sets the custom property (`1rem`, and `var(--space-4)` under `(width < 900px)`) and takes its `padding-block-end` from it (the other sides unchanged). `.tile-panel::after`: `content: ""`, absolute, `top: calc(100% + 1px)` (the 1 px is the panel's border, since `top: 100%` is the padding box's foot; its comment says so), `inset-inline: 0`, height the property, `pointer-events: none`, nothing painted. Under `(width < 900px)` its height is the property plus `env(safe-area-inset-bottom, 0px)`. If `room0` is more than the padding by over 1 px (a margin under the grid), stop and report rather than adding a second length.
- [ ] **Step 4: Watch each mutation fail, restore, `git diff` clean:** (1) no `::after`: (h) red at both widths; (2) no `pointer-events: none`: (i) red; (3) the box `width: 200vw` in place of `inset-inline: 0`: (j) red; (4) the `env()` dropped from the `(width < 900px)` height: (k) red; (5) the `env()` added at the top level too: (k) red. A fixed `1rem` height is not watched: `1rem` and `--space-4` are both 16 px here, so no measurement can tell it from the property, and review holds "named once".
- [ ] **Step 5: Whole projects:** node, jsdom, browser, `tsc -b`, biome, coverage and the floor. Quote each summary line.
- [ ] **Step 6: Commit** the two CSS files, the three test files and `web/tests/support/css.ts`: "A last-row panel ends the page with as much room below it as the last row has (O283)". Report `$E/D2-report.md` as Task 1's.

### Task 3: A leave back at the stored title clears a failure (spec item 12, added after the owner's ruling of 2026-11-14)

**Files:**
- Modify: `web/src/MetadataEditor.tsx` — `saveTitle()` (~639-652): where no save is out and the field is at the stored title (`latest.current === stored.current`, reached by a blank put-back and by the title typed back), clear `failure` before answering `null`; leave `unwritten` alone. The `again` ref's doc comment (~347-354) names the ways a failure goes; it must stay true.
- Test: `web/tests/MetadataEditor.test.tsx` (jsdom), beside "takes the failure away on the Escape that puts back the stored title" (~762).

**Interfaces:** consumes nothing new; `TitleFlush.failed()` reads `failure`, so it answers false once the failure is cleared, as it already did for that field.

- [ ] **Step 1: Write the failing jsdom cases.** Each first makes a save fail (`new Error('offline')`) and asserts the alert reads `offline`.
  - (l) `it('takes the failure away when the field is left blank after a failed save')`: clear the field, leave: field `'old'`, alert `null`, `onPatch` called once.
  - (m) `it('takes the failure away when the stored title is typed back and the field left')`: type `'old'`, leave: alert `null`, `onPatch` called once.
  - (n) `it('takes the failure away when the stored title is typed back and Enter pressed')`: as (m) by Enter.
  - (o) `it('takes the failure away when a close flushes a field back at the stored title')`: type `'old'` with the field still focused, call the ref's `flush()`: it answers `null`, the alert is `null`.
  - (p) `it('keeps the unwritten note through a leave back at the stored title')`: a first save answers not persisted, a second fails, type the first save's title back and leave: alert `null`, `.unwritten` still there.
- [ ] **Step 2: Run them; (l)-(o) fail** on the alert assertion; (p) fails on it too.
- [ ] **Step 3: Implement** in `saveTitle()` as Files says, one place, and make the `again` comment name this way out.
- [ ] **Step 4: Watch each mutation fail, restore, `git diff` clean:** (1) no clear: (l), (m), (n), (o) red; (2) the clear only when the field was blank: (m) red; (3) the clear also drops `unwritten`: (p) red; (4) the clear made before the `out.current !== null` check (so a leave while a save is out clears too): note what goes red, or say none and why it cannot matter (a save out has already cleared the failure in `send()`).
- [ ] **Step 5: Whole projects:** node, jsdom, browser, `tsc -b`, biome, the comment guard, coverage and the floor. Quote each summary line.
- [ ] **Step 6: Commit** `web/src/MetadataEditor.tsx` and `web/tests/MetadataEditor.test.tsx`: "A leave that finds the Title field back at the stored title takes a failure away, as Escape does (O283)". Report `$E/D3-report.md`.

### Finish

1. **Review** of both tasks' commits together (one reviewer, opus) against spec items 9-11, this plan and its Review Focus. Fix rounds go back to the implementer by SendMessage; review the fixes too.
2. **Gate** on the final commit by the Gate line; read `$E/gate.exit`; quote the `all green` line.
3. **The drive**, by the O283 plan's Finish 3 recipe: items 9-11 at 1280 and 390 (a save held past 200 ms by a `window.fetch` wrapper; Escape after a failed save; a last-row panel scrolled to the end), plus O283's checks 3 ("Saved" moves nothing) and 6 (the last row's panel moves no tile) again. Report `$E/drive2-report.md`.
4. **Docs:** measure the node, jsdom and browser counts on the final commit and update `docs/status.md`'s web figures; update O283's bullet only where it describes the "Saved" note, Escape after a failure, or a panel past the page's end. The PR body: the "Seen, and to be fixed…" sentence says what was fixed, the gate figures are the new gate's, and it says that `shell.css`'s `.library > main` changed (outside the spec's Constraints file list) without changing any screen's computed padding.
5. **Claim check** scoped to this plan's commits and the PR body; then push to #248 and ask the owner before merging.

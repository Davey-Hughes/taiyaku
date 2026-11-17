# Resize polish: the fade animates, covers glide, short screens get the phone layout, and the view keeps its place across the scroller swap (O303 to O306) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Continue row's fade eases in and out and halves in the phone layout, covers glide to their new columns when a list's column count changes, a screen under 900 px wide or under 500 px tall gets the phone layout wherever the layout is decided, and the view keeps its place when the scroller swaps.

**Architecture:** One module, `web/src/phoneLayout.ts`, holds the phone layout's condition and its complement; each stylesheet site writes it out in full and a node test holds every site to it. The prototype's `glide.ts` and `glideTiming.ts` become `reseat.ts` and `reseatTiming.ts`, attached from `SeriesGrid` and `Series.tsx` by a hook sharing one `ResizeObserver`, while the Continue row goes on laying itself frame by frame on the same clock. A new hook in `Shell.tsx` records the element at the top of the view after layout and puts it back in the first frame after the scroller swaps.

**Tech Stack:** React 19, TypeScript, Vitest 4 (projects `node`, `jsdom`, `browser` with two Chromium instances), Web Animations, CSS `@property`, Biome.

**Spec:** `docs/superpowers/specs/2026-11-16-resize-polish-design.md` (approved; "Settled after the builder check" holds the owner's last rulings). Prototype: the branch's first commit, "Prototype as the owner tried it", approved for feel only.

**Paths:** `WT=/home/davey/projects/taiyaku/.claude/worktrees/resize-polish`, `WEB=$WT/web`, ledger `L=/home/davey/projects/taiyaku/.superpowers/sdd/2026-11-16-resize-polish`.

## Global Constraints

- The phone layout is `@media (width < 900px) or (height < 500px)`; the rules that hold only beside the sidebar take `@media (width >= 900px) and (height >= 500px)`. Each site writes one of them letter for letter. The only other viewport-size queries are `book.css`'s `(max-width: 40rem)` and `admin.css`'s `(width < 36rem)`. No script keeps its own 900 or 500.
- `coverSizes` writes the condition as two entries, `(width < 900px)` and `(height < 500px)`, each with the filled width, before the bare size; never `or` inside `sizes`.
- The fade: `--edge-before` and `--edge-after` registered as `<number>`, not inherited, initial value 1; 200 ms `ease-out`; no transition under `prefers-reduced-motion: reduce`; stops 80, 50 and 12 px, and 40, 25 and 6 px in the phone layout; a focus ring lifts both sides at once and only the return is animated.
- The glide: 180 ms, `cubic-bezier(0.25, 1, 0.5, 1)`; a cover that starts or ends within 200 px of the viewport glides and one beyond snaps; a move under 2 px in position and in width snaps; one box read per change, and above 1.5 px off every cover in that list snaps; `transform-origin: 0 0`; `overflow-x: clip` only while the list glides.
- Snaps, never glides: a change in the number of covers (alone or with a column change); either edge of the phone layout; reduced motion, where the Continue row lays its target at once; a phone turn, while `TURN_RESEATS` is false.
- No React render per frame of a resize, a scroll or a glide. A frame's reads do not grow with the number of covers: a drag frame reads where each list is drawn, and no cover.
- Nothing at module level needs a browser: jsdom has no `matchMedia`, `ResizeObserver` or `Element.animate`, and importing any module in the jsdom project must not throw. No list is found by watching the document (no body `MutationObserver`, no document-wide `querySelectorAll`).
- In code the covers' motion is "reseat" (`reseat.ts`, `reseatTiming.ts`, `useReseat`, `RESEAT_*`, `data-reseating`); "glide" stays the reader strip's (`Filmstrip.tsx`, `tiles.ts`), so a grep for one never finds the other. Docs and test titles say "glide", as the spec does.
- Real input only. A resize is a step a frame on the test frame's box (`dragged`, now in `Library.continue.browser.test.tsx`), since `page.viewport` waits a frame and cannot pace one. A real mouse; real keys, after a click has focused the test page (else they go to the iframe). Click by `getByRole`, never by text. Navigate outside `act`, by real clicks. Assert per element, never a whole-page `toContain`.
- Reduced motion is emulated through CDP `Emulation.setEmulatedMedia`, as `Series.tile.browser.test.tsx` does; "without the glide" is the same drive under reduced motion.
- Every new test is watched failing against the defect it names, applied to the code by hand and restored. A test that passes when written (the prototype already does much of this) counts only once its defect has turned it red. Each task report lists every defect and the test it turned red; Task 12 copies them into the notes.
- Commands run from `$WEB` (`cd` with the absolute path): `./node_modules/.bin/vitest run --project <node|jsdom|browser> [files] [-t name]`, never `pnpm exec`; `--reporter=default` when reading console output; lint `./node_modules/.bin/biome check ./src ./tests`; typecheck `./node_modules/.bin/tsc -b`. From `$WT`: `node scripts/check-comments.mjs`.
- Before each commit, run the whole `node`, `jsdom` and `browser` projects, not named files, each into `$L/task-<n>-<project>.log` with its exit code in `.exit` and `/proc/loadavg` beside it; read the summary lines from the log, never pipe a run through `tail`. A red outside the task's files is judged against the Setup baseline and main, never assumed pre-existing. A browser red with no failed assertion, or the Filmstrip "Tab brings focus to the rail" case, is read against `/proc/loadavg` and the whole project rerun before it is blamed on the branch. CI never runs the browser project.
- Every new or rebuilt `web/src` module (`phoneLayout.ts`, `scrollerSwap.ts`, `reseat.ts`, `reseatTiming.ts`, and `continueRow.ts` as changed) clears `scripts/coverage-floor.mjs`'s per-file floor from the `node` and `jsdom` projects alone, which is all coverage counts: arithmetic in node tests, hooks in jsdom with the browser APIs stubbed as `continueRow.hook.test.tsx` does. Check: `./node_modules/.bin/vitest run --project node --project jsdom --coverage`, then `node scripts/coverage-floor.mjs` from `$WT`.
- A comment says why: no history, dates, measurements, "prototype" or "trial"; at most 8 lines of text a block; `web/src`'s and `web/tests`' comment shares within `scripts/comment-budget.json`. "900 px" in a comment becomes "the phone layout" where that is what it means.
- The spec's line numbers are as on main 1345a786; find each site on the branch by grep. A status row names a site by symbol, never by line.
- One commit per task, no `--amend`, no attribution lines, no scratch files in the worktree; logs and evidence go to `$L`.

## Review Focus

1. **A fast window drag that adds a column and crosses the phone layout's edge inside one glide** (a desktop window flung to under 900 px). At the crossing every animation must be cancelled and no list left marked, and O306's anchor, a cover still moving when it was last recorded, must be put back from its layout box, not from where it was drawn. Test: Task 8, F1.
2. **A real mouse drag along the Cover size slider through two column changes, a step a frame.** Covers change size and place at once and glides interrupt each other; the old geometry must be the old size's. Test: Task 7, S4.
3. **A click on a cover while covers glide, then Back.** The list unmounts mid-animation; the series page's list must be watched, the home grid watched again after Back, and nothing of the module left on `window` once no list is drawn. Test: Task 7, S5.
4. **A page scroll between two column changes** (a trackpad scroll while the window holds, then the drag goes on). The next glide must start where the scroll left the covers. Test: Task 8, F2.
5. **A page scrolled to its end crossing the phone layout's edge back and forth.** The shorter layout clamps it, and the anchor must not creep from one crossing to the next. Test: Task 5, W4.

---

### Setup (controller, before Task 1)

- `cp -a` (never symlink) `/home/davey/projects/taiyaku/web/node_modules` into `$WEB/` and `/home/davey/projects/taiyaku/crates/taiyaku-wasm/pkg` into `$WT/crates/taiyaku-wasm/`.
- Baseline on the commit that adds the spec, the branch's second: the whole `node`, `jsdom` and `browser` projects into `$L/baseline-<project>.log`, with exit codes and load. In `$L/progress.md`, list each red suite or case with its owner: a jsdom suite that throws at import (`glideTiming.ts` calls `window.matchMedia` at module level, and every suite reaching `continueRow.ts` imports it) → Task 1; an O288 fade case in `Library.continue.browser.test.tsx` (the prototype's mask always has two layers) → Task 4; a Continue row case that reads the row while it slides → Task 1. Read any other red before dispatching Task 1.
- Order, changed from the brief and why: Task 1 strips the trial first, so every later task starts from a suite whose reds are known; the fade (Task 4) comes before O306 because the prototype's mask has its tests red and Task 2 gives it the condition it needs; O304 is three tasks (6 to 8) to stay inside a task's budget; `onRowLaid`'s listeners move to Task 6, where `reseatTiming.ts` is rebuilt.

### Task 1: Strip the trial, and import nothing that needs a browser at load (budget ~60 calls)

**Files:**
- Delete: `web/src/trialLayout.ts`.
- Modify: `web/src/main.tsx` (both prototype imports); `web/src/styles/library.css` (the `data-trial-layout` rules at the end of the cover layout); `web/src/library.ts` (`coverSizes`' `?fill` branch); `web/src/glideTiming.ts` (`off`, `glideOff`, `LAYOUT_EVENT`, `isNarrow`, both module-level `matchMedia` calls); `web/src/glide.ts` (`epoch`, the `glideOff` guard, the `LAYOUT_EVENT` listener, `Geometry.narrow`); `web/src/continueRow.ts` (`turned`, `turning`, the `LAYOUT_EVENT` listener, `Laid.narrow`); "prototype" and "trial" in these files' comments.
- Create: `web/tests/support/motion.ts`.
- Test: the browser cases the baseline gave Task 1.

**Interfaces:**
- Produces: `mayGlide()` reads `prefers-reduced-motion` when called and answers false where `matchMedia` is missing. `support/motion.ts`: `frames(count: number): Promise<void>` (moved from `Library.continue.browser.test.tsx`) and `afterMotion(): Promise<void>` (waits `GLIDE_MS`, then two frames).
- `glide.ts` stays unimported, its body observer in place, until Task 6 rebuilds it.

- [ ] **Step 1: See the defect.** `./node_modules/.bin/vitest run --project jsdom tests/continueRow.hook.test.tsx`. Expected: the suite fails at import, `window.matchMedia is not a function`.
- [ ] **Step 2: Remove** what the spec's "From the prototype to the PR" lists as removed, except `glide.ts`'s body observer and `querySelectorAll` (Task 6). `glideTiming.ts` reads media only when asked. Where the Continue row and `glide.ts` asked `isNarrow`, they compare `--tile-fill` (`Laid.fill`, `Style.fill`), which is the layout drawn.
- [ ] **Step 3: Run** Step 1's command: passes. Watch: a module-level `window.matchMedia(...)` put back in `glideTiming.ts` turns every jsdom suite that renders `Library` red at import. Restore.
- [ ] **Step 4:** Each browser case the baseline gave Task 1 awaits `afterMotion()` before it reads the row: the row's slide ships, and the case is about its end state. The case changes in nothing else.
- [ ] **Step 5:** Whole projects, biome, tsc, check-comments. Only the O288 fade cases stay red (Task 4). Commit: `Strip the prototype's trial switch, and import nothing that needs a browser at load`.

### Task 2: O305, one phone layout by width or height, in every stylesheet and script (budget ~130 calls)

**Files:**
- Create: `web/src/phoneLayout.ts`; `web/tests/styles/phoneLayout.css.test.ts` (node).
- Modify, stylesheets (the spec's O305 list; on the branch, `grep -n '@media (width' web/src/styles/*.css`): `shell.css` (the phone block); `library.css` (the panel's safe-area room, a series page's gutter, `--tile-fill` and the filled columns, the search input, the half-width fade; the complement: the Continue heading); `reader.css` (the strip's 96 px); `settings.css` (the dialog's top, Configuration's gutter); `admin.css` (the complement: Rules side by side).
- Modify, scripts: `web/src/Shell.tsx` (`WIDE` goes; the drawer's query is `SIDEBAR_LAYOUT`); `web/src/tileSize.ts` (`STRIP_BREAKPOINT_PX` goes; `stripTileEdge` asks `PHONE_LAYOUT`); `web/src/library.ts` (`coverSizes`). Every comment `grep -n 900 web/src` finds that means the phone layout, in CSS and TS, including `library.css`'s pointers to `Shell.tsx`'s `WIDE` and to `coverSizes` as the one script repeating the breakpoint.
- Modify, the ties to the constants: `tests/styles/library.css.test.ts` (:30, :41, and :75 to :79, which also checks `coverSizes` against the fill block), `tests/styles/settings.css.test.ts` (:34, :42), `tests/styles/reader.css.test.ts` (:89, O148's pin), `tests/library.test.ts` (:359 to :360), `tests/Library.test.tsx` (:1296 to :1297), `tests/tileSize.test.ts` (:49), `tests/Shell.test.tsx` (:1099), `tests/Filmstrip.browser.test.tsx` (:30, :233), `tests/builtStylesheet.test.ts` (a new case).
- Modify, cases that shrink a window 900 px or wider under 500 px tall: `tests/Shell.browser.test.tsx` (:1381), `tests/Libraries.shell.browser.test.tsx` (`atTheFoot` at :667, the case at :1146). This plan's grep of every `viewport(` call (variables traced to their values) found no other; run it again.

**Interfaces:**
- Produces `web/src/phoneLayout.ts`, the first place this code exists:
  ```ts
  export const PHONE_LAYOUT_PARTS = ['(width < 900px)', '(height < 500px)'] as const
  export const PHONE_LAYOUT = PHONE_LAYOUT_PARTS.join(' or ')
  export const SIDEBAR_LAYOUT = '(width >= 900px) and (height >= 500px)'
  ```
- `coverSizes(size)` keeps its signature; at 144 it answers `(width < 900px) 320px, (height < 500px) 320px, 144px`.

- [ ] **Step 1: Confirm H.** Before anything says 500: the landscape heights, browser bars shown and hidden, of current large phones (the owner's is 440 px); the smallest landscape heights of current tablets; and which mobile browsers resize the layout viewport for a soft keyboard (`index.html` sets no `interactive-widget`). Each figure with its source goes in `$L/h-figures.md`. Every phone under 500 and every tablet over it: go on. Otherwise stop and report, since every size in this plan follows H.
- [ ] **Step 2: Tests first.** Create `phoneLayout.ts` as above. `phoneLayout.css.test.ts` reads `src/index.css` and every `.css` it finds by listing `src/styles/`: each `@media` whose prelude tests the viewport's width or height is `@media ${PHONE_LAYOUT}`, `@media ${SIDEBAR_LAYOUT}` or one of the two named exceptions, letter for letter; each of the two forms occurs; the exceptions list holds exactly those two. Move every tie above to the constants; :75 to :79 becomes: the fill block's condition is `PHONE_LAYOUT`, and `coverSizes` names each of `PHONE_LAYOUT_PARTS` in order with the same filled width before the bare size. `builtStylesheet.test.ts`: the built stylesheet's viewport-size `@media` preludes, whitespace removed, hold both forms and nothing outside the four.
- [ ] **Step 3: Run** `--project node tests/styles tests/library.test.ts tests/tileSize.test.ts tests/builtStylesheet.test.ts` and `--project jsdom tests/Library.test.tsx tests/Shell.test.tsx`. Expected: every new or moved assertion fails on the width-only stylesheets and scripts.
- [ ] **Step 4: Implement** every site, the three scripts on the module, the comments. If Lightning CSS rewrites the condition (the sources pass and the built case fails), stop and report.
- [ ] **Step 5: The cases that shrink under 500 px.** Shell :1381 moves to a height at or above 500 that is still shorter than the account row's depth, which the case already checks. `atTheFoot` and :1146 keep the window at or above 500, deepening the page where the case could not otherwise put the control below the fold. Each still checks what it checked.
- [ ] **Step 6: Run** Step 3's files: pass. Watch: a phone-side site, then the complement site, written width-only fails the agreement case naming it; a `cssTarget` entry for a browser without media range syntax fails the built case. Restore.
- [ ] **Step 7:** Whole projects, biome, tsc, check-comments. Commit: `A screen under 500 px tall gets the phone layout, by one condition the stylesheets and scripts share (O305)`.

### Task 3: O305, the phone layout in landscape, checked in a browser (budget ~90 calls)

**Files:**
- Test: `web/tests/Shell.browser.test.tsx` (a new describe, on `at`, `shellAt` and `openDrawer`); `web/tests/coverLayout.browser.test.tsx` (the "below 900 px (O285)" describe becomes "in the phone layout" and gains 956×440); `web/tests/Library.continue.browser.test.tsx` (the Continue heading cases); `web/tests/Cover.browser.test.tsx` (`a cover's image`; its `drawn` takes a height); `web/tests/Filmstrip.browser.test.tsx` (the rung cases); `web/tests/SearchBar.browser.test.tsx`.
- No production change is expected; a red that needs one goes back to Task 2's site.

**Interfaces:** consumes `PHONE_LAYOUT`, `SIDEBAR_LAYOUT`.

- [ ] **Step 1: Tests**, with real input:
  - **P1** `agrees on the layout at %ix%i`: at 899×800, 900×800, 1280×499, 1280×500, 956×440, 440×956 and 1280×800, these four are all true or all false: `matchMedia(PHONE_LAYOUT).matches`; `.library`'s `--tile-fill` is 1; `.phone-bar` is drawn; the document scrolls and `main`'s `overflow-y` is not `auto`.
  - **P2** `closes the drawer as the window leaves the phone layout by %s`: opened by a real click on ☰ at 899×800 then 900×800; at 1280×499 then 1280×500; at 956×440 then 956×800.
  - **P3** at 956×440: a real click on ☰ opens the drawer and focus moves to its first link; the document scrolls and `main` does not.
  - **P4** at 956×440: the covers fill their columns; the Continue heading has its phone margin; a cover asks for the filled width's image (the expected `?w=` worked out from the drawn width as the 390 px row does, never typed); the strip asks for and draws the narrow rung.
  - **P5** `keeps the search field's focus and text through a turn by height`: at 1280×800 the box focused by a real click and a word typed by real keys; at 1280×440 and back it is the same element, focused, holding the word.
- [ ] **Step 2: Run** each file `--project browser`. Expected: they pass (Task 2 built it).
- [ ] **Step 3: Watch** each against its site written width-only, then restored: P1 against `shell.css`'s phone block and `library.css`'s fill block in turn; P2 against `Shell.tsx`'s query; P3 against `shell.css`; P4 against the fill block, the Continue heading's complement, `coverSizes` and `reader.css` in turn; P5 against the field remounted at the turn (a `key` that follows the layout).
- [ ] **Step 4:** Whole projects, biome, tsc. Commit: `The phone layout in landscape: the drawer, the document scrolling, filled covers and their images, the strip and the search field, held at both edges (O305)`.

### Task 4: O303, the Continue row's fade eases in and out (budget ~80 calls)

**Files:**
- Modify: `web/src/styles/library.css` (the `:has(:focus-visible)` rule gains `transition: none`, so the change into it is immediate and the base rule's 200 ms carries the change back; the comment above the `@property` rules moves to the rule it describes). The rest of the prototype's fade is kept; its half-width block is Task 2's.
- Test: `web/tests/Library.continue.browser.test.tsx` (`describe('the Continue row's fade (O288 item 3)')`, its mask reader and the `NONE`, `AFTER`, `BOTH`, `BEFORE` shapes).

**Interfaces:** none.

- [ ] **Step 1: Tests first.** The mask reader answers, per side, the alpha at its first stop and its stops; the mask always has two layers, and a side that does not fade reads alpha 1 at every stop. Restate the O288 cases on it, each read once the fade has come to rest; "lifts the fade while a card has a focus ring" expects alpha 1 on both sides, not `none`. New:
  - **M1** `fades in and out over 200 ms`: about 100 ms after a side starts or stops fading, its alpha is strictly between 0 and 1; at 200 ms and a frame it is at its end.
  - **M2** `lifts the fade at once for a focus ring, and brings it back over 200 ms`: a real Tab into the row: both sides at alpha 1 in the first frame; focus leaves: between 0 and 1 at about 100 ms, at its end by 200 ms and a frame.
  - **M3** `flips at once under reduced motion`: `transitionDuration` is `0s` and the alpha flips in the next frame.
  - **M4** `fades 6, 25 and 40 px at %s`: 390×844 and 956×440; and 12, 50 and 80 px at 1280×800.
- [ ] **Step 2: Run** `--project browser tests/Library.continue.browser.test.tsx -t fade`. Expected: M2 fails (the prototype's transition runs into the focus state); the rest pass.
- [ ] **Step 3: Implement** the focus rule's `transition: none` and the comment's move.
- [ ] **Step 4: Run**: pass. Watch: M1 against the `@property` rules removed; M2 against `transition: none` removed; M3 against the reduced-motion block removed; M4 against the half-width block written width-only. Restore.
- [ ] **Step 5:** Whole projects (the browser project is green from here on), biome, check-comments. Commit: `The Continue row's fade eases in and out over 200 ms, half as wide in the phone layout, and a focus ring lifts it at once (O303)`.

### Task 5: O306, the element at the top stays there when the scroller swaps (budget ~140 calls)

**Files:**
- Create: `web/src/scrollerSwap.ts`; `web/tests/scrollerSwap.hook.test.tsx` (jsdom).
- Modify: `web/src/Shell.tsx` (one call, on `main`'s ref); `web/src/scrollKeeper.ts` (export `scroller`; whatever W5 needs); `web/tests/support/motion.ts` (`ramp` and `dragged` move here from `Library.continue.browser.test.tsx`, whose callers follow).
- Test: `web/tests/scrollKeeper.browser.test.tsx` (a new describe on its `app` harness, given a height).

**Interfaces:**
- Produces: `useScrollerSwap(main: RefObject<HTMLElement | null>): void`, called once, in `Shell`. `dragged(sizes: readonly { width?: number; height?: number }[]): Promise<{ width: number; height: number }[]>`: a step that leaves out a side keeps it; it answers the size each frame drew.
- Consumes: `scroller()` from `scrollKeeper.ts` (which element scrolls, read from `main`'s `overflow-y`).

- [ ] **Step 1: Tests first** (browser), at 1280×800, the page scrolled to its middle by a real wheel. The anchor: inside a cover list, the first cover on the top row seen; elsewhere, the element at the top edge; in the phone layout the top is under the phone bar (`--sticky-top`).
  - **W1** `keeps the element at the top through a width drag to 880 px and back, on %s`: the home grid, search hits, a series page, a Configuration screen; a step a frame; in the first frame of each layout the anchor is within 1 px of the top.
  - **W2** the same through a height drag to 480 px and back.
  - **W3** `keeps the same anchor over ten crossings`, and `leaves a page at its top at its top`.
  - **W4** (Review Focus 5) `holds a page scrolled to its end over ten crossings`: where a layout cannot scroll that far the page rests at its end, and from the second round trip on, each layout's view is within 1 px of its view the time before.
  - **W5** `lands Back after a swap at the place put back`: cross, open a series by a real click, Back: the anchor at the top within 1 px.
  - jsdom: the hook records, puts back on a swap the stubbed observer reports, and lets go on unmount.
- [ ] **Step 2: Run** `--project browser tests/scrollKeeper.browser.test.tsx -t <the describe's name>` and the jsdom file. Expected: W1 to W5 find the view at 0; the jsdom file fails to import the hook.
- [ ] **Step 3: Implement.** Record the anchor ahead of the swap, after layout (a `ResizeObserver` callback, or a scroll seen at the next frame); never read layout in a frame the window resized in before its observers have run, which lays the page out at the new size. A swap is a change in `scroller()`. Put the anchor back in the observer callback, before the first paint in the new layout. The anchor's place is read from layout, never from a box a transform moves (Task 8, F1).
- [ ] **Step 4: Run**: pass. Watch: W1 to W4 against the restore removed; W5 against the swap's 0 saved under the history entry (the scroller's top saved before the restore); the jsdom case against the unmount's cleanup removed. Restore.
- [ ] **Step 5:** Whole projects, the coverage floor for `scrollerSwap.ts`, biome, tsc, check-comments. Commit: `A window crossing into or out of the phone layout keeps what was at the top of the view, and Back lands there (O306)`.

### Task 6: O304, the covers' motion becomes a hook on the lists that have covers (budget ~140 calls)

**Files:**
- Rename: `git mv web/src/glide.ts web/src/reseat.ts`, `git mv web/src/glideTiming.ts web/src/reseatTiming.ts`, and their identifiers.
- Modify: `web/src/reseat.ts` (the module-level `ResizeObserver`, the body `MutationObserver`, `watchAll` and its `querySelectorAll` go; the hook takes their place); `web/src/reseatTiming.ts` (`onRowLaid` keeps many listeners); `web/src/SeriesGrid.tsx` (`.grid`); `web/src/Series.tsx` (`.volumes`, the grid layout only); `web/src/continueRow.ts` (imports); `web/src/styles/library.css` (`data-gliding` becomes `data-reseating`); `web/tests/support/motion.ts` (`afterMotion` waits `RESEAT_MS`).
- Create: `web/tests/reseat.test.ts` (node), `web/tests/reseat.hook.test.tsx` (jsdom), `web/tests/reseatTiming.browser.test.tsx`.
- Modify: each existing browser case that reads a cover's box while it moves (likely in `coverLayout`, `Series.panel` and `Library.continue`; every one the whole run turns red).

**Interfaces:**
- Produces `reseatTiming.ts`: `RESEAT_MS = 180`; `RESEAT_EASING = 'cubic-bezier(0.25, 1, 0.5, 1)'`; `reseatEased(progress: number): number`; `NEAR_PX = 200`; `SNAP_PX = 2`; `frameTime(): number`; `mayReseat(): boolean`; `onRowLaid(listener: () => void): () => void`, answering the listener's removal; `rowLaid(): void`, calling every listener.
- Produces `reseat.ts`: `useReseat(list: RefObject<HTMLUListElement | null>, drawn: boolean): void`. The shared observer, the window listeners and the module's `onRowLaid` listener start with the first list and stop with the last. A list with a cover in motion carries `data-reseating`.
- Kept from the prototype: the arithmetic (`styleOf`, `geometryOf`, `jumped`, `start`, `progressOf`), exported where the node test needs it; the start time set to `frameTime()`; the self-check. A turn of the layout is read from `--tile-fill` in `styleOf`.

- [ ] **Step 1: Tests first.**
  - node `reseat.test.ts`: `geometryOf` against hand-worked lists (Smooth and filled, one column and several, rows the grid did not lay giving `undefined`); the move per cover: under 2 px in position and width is none, one beyond 200 px of the view before and after is none, a running move's progress carries into the next start; `onRowLaid`: two listeners both called, a removed one not.
  - jsdom `reseat.hook.test.tsx`, with the browser APIs stubbed: two lists share one observer, which stops with the last; window listeners added with the first list and removed with the last (spies); `drawn` false observes nothing.
  - browser **E1** `the animation's curve and reseatEased agree`: an element animated with `RESEAT_EASING` over `RESEAT_MS`, paused at several times: its computed progress within 0.001 of `reseatEased`.
- [ ] **Step 2: Run** the node and jsdom files. Expected: they fail on names that do not exist yet. E1 passes when written.
- [ ] **Step 3: Implement** the renames, the hook, its two attachments and the attribute. The existing browser cases that read a list now in motion await `afterMotion()`; none turns motion off for a file, and each such case changes in nothing else (no assertion loosened, moved or dropped). The task report lists every case so changed, for the reviewer to diff.
- [ ] **Step 4: Run**: pass. Watch: E1 against `RESEAT_EASING` changed alone; `onRowLaid` against a single slot; the hook case against the observer kept past the last list; the jsdom suites that render `Library` and `Series` against a module-level `new ResizeObserver` in `reseat.ts`. Restore.
- [ ] **Step 5:** Whole projects, the coverage floor for `reseat.ts` and `reseatTiming.ts`, biome, tsc, check-comments. Commit: `Library covers glide to their new columns, attached from the lists that draw them, with nothing running before one is drawn (O304)`.

### Task 7: O304, a glide starts where the covers are drawn and ends clean (budget ~120 calls)

**Files:**
- Create: `web/tests/reseat.browser.test.tsx` (the home grid by `homePage`, a series page by `seriesPage`).
- Modify: `web/src/reseat.ts` only where a case finds a defect, its red recorded first.

**Interfaces:** consumes `useReseat`, `RESEAT_MS`, `data-reseating`, `dragged`, `afterMotion`.

- [ ] **Step 1: Tests first**, on both pages. Where a cover is drawn is its layout box plus its running transform, both taken at the glide's start time, as the glide report's "The checks" measured it.
  - **S1** `starts where drawn`: in the frame a glide starts, every cover on screen is within 0.5 px of where the frame before drew it: one column each way; a reversal 3 frames into a glide; a second change 4 frames in.
  - **S2** `ends clean`: `RESEAT_MS` and two frames after the last change, no cover has an animation, every computed transform is `none`, no list carries `data-reseating`, and every box is within 0.5 px of the reduced-motion drive's; the same after a chip filter changes the list mid-glide.
  - **S3** `glides on a Cover size slider step that adds a column`: an arrow key, the page focused first: a cover has an animation, then S2's end.
  - **S4** (Review Focus 2) `starts where drawn through a mouse drag along the slider`: a real mouse, a step a frame, through two column changes: S1's bound at each, S2's end.
  - **S5** (Review Focus 3) `follows a click on a cover mid-glide, and Back`: a real click on a cover's link while it moves; on the series page no cover has an animation and its list glides on the next column change; after Back the home grid glides again; on a page with no cover list none of the module's window listeners remains (spies).
  - **S6** `renders nothing through a glide`: `homePage`'s `onCommit` is not called from the drive's first step to S2's end.
- [ ] **Step 2: Run** the file `--project browser`. Expected: pass on the prototype's arithmetic; a case that fails is a defect to fix in this task.
- [ ] **Step 3: Watch**: S1 against the start time left to the browser, then against the old box taken without the running glide; S2 against a final keyframe other than `none`, then against `fill: 'forwards'`; S3 against the list's tokens read only when its width changes; S4 against the old geometry worked out with the new size's tokens; S5 against the observer not made again once the last list has gone; S6 against a state the hook sets at each change. Restore.
- [ ] **Step 4:** Whole projects, biome, tsc. Commit: `A glide starts where each cover is drawn, mid-glide, mid-drag and after a click included, and leaves nothing behind (O304)`.

### Task 8: O304, what snaps, what a frame reads, and the self-check (budget ~120 calls)

**Files:**
- Modify: `web/src/reseat.ts` (the capturing `scroll` listener stays only if F2 needs it; what F1 needs, with `web/src/scrollerSwap.ts`).
- Test: `web/tests/reseat.browser.test.tsx`.

**Interfaces:** consumes Tasks 5 to 7.

- [ ] **Step 1: Tests first.**
  - **N1** `snaps across the phone layout's edge by width`. **N2** `snaps across it by height where the column count holds`: first measure that 240 px covers lay the same number of columns at 1280×800 and 1280×480 (if not, find a width where they do and use it), then cross by height: no animation. **N3** a cover beyond 200 px of the view before and after has no animation while one on screen has. **N4** under reduced motion no animation through a drag over several column counts. **N5** `snaps when the number of covers changes`: a chip filter alone, and a chip filter landing in the same frame as a column change: no cover has an animation.
  - **R1** `reads as much per frame at the fixture's covers as at about 500`: `getBoundingClientRect` and `getComputedStyle` spied and counted per frame over a 60-frame drag with no column change. **R2**, only if the scroll listener stays: the reads per scroll event, the same at the fixture's covers and at about 500.
  - **C1** `snaps a list not laid where its arithmetic says`: a style the arithmetic does not model, added by the test, puts the probe more than 1.5 px off: no cover animates at the change, and `console.warn` is called once (the self-check also logs, as `MetadataEditor.tsx` logs an impossible state).
  - **F1** (Review Focus 1) `cancels a glide the phone layout's edge interrupts, and keeps the anchor`: the home grid scrolled to its middle; a drag a step a frame from a few frames above the nearest width over 900 px where a column comes or goes (measured), on to 880 px, so the crossing lands inside the glide: in the first frame of the phone layout no cover has an animation, no list carries `data-reseating`, and the anchor is within 1 px of the top.
  - **F2** (Review Focus 4) `starts where drawn after a scroll between two changes`: a column change; a real wheel scroll of `main` while the window holds; the drag on to the next change: S1's 0.5 px.
- [ ] **Step 2: Run**. Expected: pass on the prototype, except where the swap meets a running glide (F1).
- [ ] **Step 3: Implement** what F1 and F2 show. The scroll listener stays only if F2 fails without it, and then R2 holds it.
- [ ] **Step 4: Watch** each against its condition removed in turn: N1 and N2 against the layout-edge snap; N3 against the margin; N4 against `mayReseat`'s reduced-motion check; N5 against the count check removed; R1 against each cover read every frame; R2 against the listener reading every cover; C1 against the self-check removed; F1 against the anchor read from the drawn box; F2 against whatever keeps the start through a scroll. Restore.
- [ ] **Step 5:** Whole projects, the coverage floor, biome, tsc, check-comments. Commit: `Covers snap where a glide would mislead: across the phone layout's edge, far off screen, under reduced motion, or on a list not laid as reckoned (O304)`.

### Task 9: The Continue row moves with the covers, and the page holds still (budget ~130 calls)

**Files:**
- Modify: `web/src/continueRow.ts` (a column change slides the row only where it lies on screen or within `NEAR_PX` of it, and otherwise lays the target at once, leaving the browser one correction to make; the rest of the prototype's slide is kept); `web/src/reseat.ts` if a still-page case needs it.
- Test: `web/tests/Library.continue.browser.test.tsx`, `web/tests/continueRow.hook.test.tsx`.

**Interfaces:** consumes `reseatEased`, `RESEAT_MS`, `NEAR_PX`, `frameTime`, `mayReseat`, `rowLaid`.

- [ ] **Step 1: Tests first** (browser, home grid):
  - **K1** `keeps card k over the grid's cover k through a glide`: an uninterrupted glide, the row at its start: in every frame card k's left edge is within 0.5 px of the first row's cover k.
  - **K2** `scrolls nothing sideways in any frame of a glide`: at 1280 px and in the phone layout, in no frame do `main` or the document scroll sideways.
  - **K3** `holds main's scroll through a glide`: in Smooth, scrolled down in `main`, every frame's scroll position equals the reduced-motion drive's.
  - **K4** `rests where the reduced-motion drive rests in the phone layout`: scrolled down in the document so the row lies far above the view, a column change: the page rests within 1 px of the reduced-motion drive.
  - jsdom: far from the view the row lays its target at once; near it, it slides.
- [ ] **Step 2: Run**. Expected: K4 and the jsdom case fail (the prototype slides the row wherever it lies); the rest pass.
- [ ] **Step 3: Implement** the rule above.
- [ ] **Step 4: Run**: pass. Watch: K1 against `RESEAT_EASING` changed without `reseatEased`; K2 against the sideways clip removed; K3 against the Smooth target's column taken from the filled column (a card that changes width in Smooth); K4 and the jsdom case against the row sliding while far off screen. Restore.
- [ ] **Step 5:** Whole projects, the coverage floor for `continueRow.ts`, biome, tsc, check-comments. Commit: `The Continue row moves with the covers on their clock and curve, and the page holds still through a glide (O304)`.

### Task 10: An open volume panel stays on top through a glide (budget ~60 calls)

**Files:**
- Modify: `web/src/styles/library.css` (while its list carries `data-reseating`, the tile holding `.tile-panel` is lifted above its neighbours: a moving tile is a stacking context, so the panel's `z-index: 5` counts only inside it); `web/src/tilePanel.ts` (`layPanel` reads the tile's box without its transform).
- Test: `web/tests/Series.panel.browser.test.tsx`; `web/tests/tilePanel.test.tsx` for the floor.

**Interfaces:** `layPanel(panel, tile)` keeps its signature.

- [ ] **Step 1: Tests first** (series page, a volume's panel opened by a real click):
  - **V1** `stays over the covers mid-glide`: a column change; in a mid-glide frame, `document.elementFromPoint` at the panel's centre is inside the panel.
  - **V2** `is laid where the reduced-motion drive lays it after a glide`: its left and width within 0.5 px of that drive's.
- [ ] **Step 2: Run**. Expected: both fail (the covers after the tile paint over the panel; the observer in `Series.tsx` lays it from the moved box during the drag).
- [ ] **Step 3: Implement**.
- [ ] **Step 4: Run**: pass. Watch: V1 against the lift removed; V2 against `layPanel` reading the transformed box. Restore.
- [ ] **Step 5:** Whole projects, biome, tsc, check-comments. Commit: `An open volume panel stays above the covers through a glide, laid from its tile's place rather than where the tile is drawn (O304)`.

### Task 11: A phone turn snaps, decided by one constant (budget ~70 calls)

**Files:**
- Modify: `web/src/reseatTiming.ts` (`TURN_RESEATS` and the turn signal); `web/src/reseat.ts` and `web/src/continueRow.ts` (a turn snaps unless `TURN_RESEATS`).
- Test: `web/tests/reseat.browser.test.tsx`; node or jsdom cases for the floor.

**Interfaces:** Produces `export const TURN_RESEATS = false`, the one place a turn's motion is decided; the hand-check's second build flips it.

- [ ] **Step 1: Choose the signal**: `screen.orientation`'s `change`, an `(orientation: …)` `MediaQueryList`, or the two dimensions swapping; and its order against the lists' observer callbacks. The app runs inside Vitest's frame: `(orientation: …)` follows the frame's box, `screen.orientation` the CDP override, so the test changes both in one frame and first checks that the frame sees both. The choice and its reason go in the task report, for the notes.
- [ ] **Step 2: Tests first.** **T1** `snaps the covers and the Continue row on a turn`: the home grid with a Continue row at 440×956, turned by CDP `Emulation.setDeviceMetricsOverride` with `screenOrientation` landscape and the frame's box set to 956×440: in the six frames after, no cover has an animation and the row is laid at its target. **T2** `glides a column change that is not a turn`: a slider step at 956×440 glides.
- [ ] **Step 3: Run**. Expected: T1 fails (a turn is a column change inside the phone layout, so the covers glide); T2 passes.
- [ ] **Step 4: Implement**. Watch: T1 against the turn's check removed; T2 against the turn's check firing on any resize. Restore.
- [ ] **Step 5:** Whole projects, the coverage floor, biome, tsc, check-comments. Commit: `A phone turned sideways snaps its covers into place, by one constant the hand-check can flip (O304)`.

### Task 12: The documents, the Firefox run and the gate (budget ~110 calls)

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `docs/superpowers/specs/2026-09-17-style-pass-design.md` (§2's **Breakpoint**, §7.1), `docs/superpowers/specs/2026-11-14-cover-size-and-continue-row-design.md` (item 9, Non-goals).

- [ ] **Step 1: `status.md`.** O303 to O306 are opened and closed in this PR: rows after O302 in its style (`[x]`, a closing statement, sites by symbol) and a bullet each below the table. O307 (the open drawer's width, `min(20rem, 85vw)` against the sidebar column's `14rem`, now reached on a phone in landscape) and O308 (a kept scroll, a pixel offset per history entry and the reader's way out, restored into another layout) are opened, `[ ]`, with bullets. Then the open-items ritual's counts: the total count word at the head of "Open items", the closed list and count, "leaving N", the "What remains" line (its count word wraps: grep the opener with `-A1`), and the web test clause measured from whole `node` and `jsdom` runs, never added to. The Rust count does not move.
- [ ] **Step 2: Sweep**: the ritual's four greps; every table row has exactly six `|`; the bare O303 to O308 across `docs/` and `web/src/`, every hit read; `STRIP_BREAKPOINT_PX`, `WIDE`, `glide.ts` and `glideTiming.ts` across `docs/` (closed rows and history left as written) and `web/`.
- [ ] **Step 3: Firefox, once.** Build the head (`node $WEB/node_modules/vite/bin/vite.js build $WEB --outDir <scratchpad>/dist`); serve it from a scratch copy of the test library by the live-server recipe (the port checked free first, the server killed by PID after); drive Playwright's headless Firefox (`$WEB/node_modules/playwright`) from a throwaway script in the scratchpad, as the glide report's "The checks" did: the start error, the end against the same drive under `reducedMotion: 'reduce'`, and the still page in Smooth and in the phone layout. Nothing of it enters the worktree.
- [ ] **Step 4: `notes.md`**: a section `## O303 to O308: the fade, the glide, the phone layout by height and the place across the swap (2026-11-16)`: H and its reasoning with `$L/h-figures.md`'s figures and the keyboard finding; the turn signal chosen; Step 3's figures; each defect a test was watched failing against, from the task reports. Only figures measured or pinned by a test.
- [ ] **Step 5: Dated lines**, one italic line each, as O302's in the 2026-11-14 spec's item 9: in the style-pass spec's §2 ("Breakpoint") and §7.1, and in the 2026-11-14 spec's item 9 and Non-goals, saying that since 2026-11-16 (O305) the phone layout holds below 900 px wide or below 500 px tall, with a pointer to O305's row. Closed rows are not edited.
- [ ] **Step 6:** check-comments; commit `Open and close O303 to O306, open O307 and O308, and the notes for them`. Then the gate on that commit, from `$WT`: `env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $L/gate1.log 2>&1; echo $? >| $L/gate1.exit`, with `git -C $WT rev-parse --short HEAD >| $L/gate1.head` and `cat /proc/loadavg >| $L/gate1.load`. Quote its `all green` line, or the failing step's lines from the log.

### Finish

1. **Whole-branch review** of `main..HEAD` against the spec and the Review Focus, its fix rounds reviewed too; after a fix, grep for the finding's shape.
2. **Gate** again, as Task 12 Step 6, if anything changed after `gate1.head`.
3. **Hand-check** by the owner on the iPhone 17 Pro Max over the tailnet. Two builds of the final head: as it stands, and a detached worktree of the head in the scratchpad (its own copied `node_modules` and wasm `pkg`) with `TURN_RESEATS = true`. Each is served by the live-server recipe from its own scratch copy of the test library, on its own port with `--log-requests`, and put on the tailnet with `tailscale serve --bg --https=<port> http://127.0.0.1:<port>`, turned off afterwards since it outlives the session. Read each answer's user-agent in the log before believing it. The list is the spec's "Testing" hand-check, plus which turn the owner picks: if the glide, `TURN_RESEATS` flips and T1 goes, then the gate again.
4. **Claim check** of the PR body, the status rows and the notes before saying "ready to merge", and after each correction.
5. **PR** on Gitea (the MCP, not `gh`): a title under 255 characters in the house style, the body not hard-wrapped, no attribution.

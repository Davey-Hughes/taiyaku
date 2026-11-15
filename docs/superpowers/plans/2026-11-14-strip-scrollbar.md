# The strip's progress line as a scrollbar: O286 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hold the prototype the owner settled (6ebbebbc) with tests watched failing, fix what the spec leaves to the builder, and close O286: a mouse drags the mark on the reader's progress line or presses the line to jump, the mark is sized like a browser's thumb between 64 and 160 px, and the line wakes thicker at a mouse's move.

**Architecture:**
- Task 1: the baseline. The pointer table learns the line's condition, the band and the wake ask one condition, the comment guard goes green, and the tests the prototype made false are rewritten to the 64–160 px rule.
- Task 2: drag and jump, both directions, long and short volume; the declarations `placeMark` reads come before it.
- Task 3: the 10 px press band, who it answers, and what a press leaves alone.
- Task 4: a hold while the rail's length changes; the long drag measured; the 4.4 px sample chased.
- Task 5: wake and sleep, and what the bars' going leaves behind.
- Task 6: how the line is drawn and eased, and the Title field's failure colour.
- Task 7: `docs/status.md`, `docs/notes.md`, and pointers in O216's and O138's records.

Web only: no server, API or route change.

**Tech Stack:** TypeScript/React 19 (`web/`); the vitest projects `node`, `jsdom` and `browser` (Chromium; `*.scrollbars.browser.test.tsx` runs in a second launch that shows scrollbars). Playwright's Firefox (`web/node_modules/playwright`) for the Finish's drive.

**Spec:** `docs/superpowers/specs/2026-11-14-strip-scrollbar-design.md`, approved by the owner, committed at dfd77643 on `o286-strip-scrollbar`. The prototype as the owner tried it is 6ebbebbc (`Chrome.tsx`, `Filmstrip.tsx`, `styles/reader.css`, `styles/library.css`); this plan holds it and does not re-design it. Its six rounds' check scripts, `check1.mjs`–`check6.mjs` and `probe5.mjs` with their logs, are in `/tmp/claude-1000/-home-davey-projects-taiyaku/5b6b6814-b86e-4813-81c7-d785d93dc0ca/scratchpad/o286/`: a reference for what was measured, read-only. Line numbers below are hints at dfd77643; locate by symbol.

**Gaps found against the prototype** (each settled in the task named):
1. **The wake's condition.** The prototype asks `(hover: hover) and (any-pointer: fine)` for the wake, in the `matchMedia` call and in `reader.css`'s thickening block, and `(any-pointer: fine)` for the band. The spec's call (goal 6) is that both exist wherever `any-pointer: fine` holds. Written so. The two part only on a device whose primary pointer cannot hover but which has a mouse or trackpad too, where the band would take presses on a line that never woke. Task 1.
2. **`pointerRulesAs` throws on the line's rules.** `HOLDS` (`tests/support/pointer.ts`) names neither condition, so every browser file that switches pointer rules with `index.css` loaded (About, App, Config, Licence, RuleEditor, the Series files, Shell) throws at the prototype. Task 1.
3. **The comment guard is red at the prototype**: the blocks above `LINE_WAKE_IDLE_MS` and above `scrollerRef` in `Chrome.tsx` (9 lines each) and the one above the thickening `@media` in `reader.css` (14) pass the cap of 8. Task 1.
4. **A press on the band where the mark is hidden** returns before `preventDefault`, so it moves focus off what had it and can start a selection. The band's top 4 px were the rail's padding before. Every primary mouse press on the band is prevented, mark or no mark (Anti-goals, first line). Task 3.
5. **`placeMark` reads `lineHeld` and calls `scrollLineTo`, both declared after it.** They are declared before it. Task 2.
6. **The bars going mid-hold.** `Chrome` returns `null` and stays mounted, so `lineHeld` outlives the line it names, and `wakeOnMouse`'s `held()` then keeps the next line awake for good. The spec's call: the hold ends as a release would, and the line comes back at rest. Task 5.
7. **The 4.4 px sample.** check5's long left-to-right drag of the mark (1280×800, 237 pages), read two frames after each move, once found the mark 4.4 px off the pointer. probe5's drags of the same volume saw the rail's width go from 27,952 to 27,845 px. That 0.38 % of the line's 1,192 px of travel is about 4.6 px. The likely mechanism: a window rebuild's render tells a share from the changed length, and the mark is written there for a painted frame before `placeMark`'s re-stand lands. Task 4 confirms or refutes it.

## Global Constraints

- **Comments.** A comment says why: the reason, the invariant, what breaks if it changes. Never history or measurements; it may point at an O-number. No ISO date unless it follows `ruling of`. A new comment block holds at most 8 lines of text. `node scripts/check-comments.mjs`, run from the repo root, checks this; if it fails on an area's comment share, follow `docs/ci.md`, "The comment guard", and say so in the report. A prototype comment that a task's tests show false is rewritten by that task: "while a mouse is on the line", "only where there is a mouse", "look as they did", "a mouse that hovers".
- **Attribution.** None anywhere: code, comments, docs, commits.
- **Pinned values** (spec, Goals):
  - Mark width: `clamp(64px, <share × 100>%, 160px)` of the line's width, the share being `share.end - share.start`.
  - Drag: the mark stays under the pointer at the point it was taken (`grab`, a fraction of the mark's width). A press on the line off the mark centres the mark on the pointer (`grab` 0.5) and goes on as a drag. Past either end the rail stops at that end. No glide after a release. A press stops a glide the rail is running.
  - Press: only `pointerType === 'mouse'` with `button === 0`. The band is 10 px tall, centred on the line (4 px above and below the 2 px line), across the line's width. It exists only under `@media (any-pointer: fine)` and is `display: none` elsewhere. The 16 px keep zone takes no press.
  - Wake: any `mouse` `pointermove` or `pointerup` over the window while the bars are up sets `data-awake` on `.reader-progress`. It goes 1000 ms after the last one, except while the pointer rests no more than 16 px above the line's top or below its bottom, anywhere across `.reader-bottom-bar`'s width, or while a hold is on (`data-held`). `matchMedia('(any-pointer: fine)')` is asked once per show.
  - Thickness: asleep, the line, its fill and the mark are 2, 2 and 4 px; awake, 4, 4 and 8. The line's extra 2 px are two 1 px `box-shadow`s, so nothing is laid out. All three are centred on the line, and nothing else in the bar moves.
  - Motion: one registered property, `@property --grow { syntax: "<number>"; inherits: true; initial-value: 0 }`. Waking is `transition: --grow 150ms ease-out` and sleeping `--grow 300ms ease-in-out`. It is `transition: none` under `[data-still]` (`!isAnimating(motion)`) and under `(prefers-reduced-motion: reduce)`.
  - Fill: its leading end has a radius of half its height (1 px asleep, 2 awake), and its start corners are square. Nothing is drawn at 0 %, and at 100 % it stays inside the line. For a work read right to left the line is `scaleX(-1)`, which puts the round end on the left.
  - The line stays `aria-hidden="true"`, takes no focus and answers no key.
  - `.metadata-editor .failure { color: var(--danger) }`.
- **Web checks per task.** Run from `web/` by absolute path, never a bare `cd` (it is zoxide here), never `pnpm exec`. `E=/home/davey/projects/taiyaku/.superpowers/sdd/strip-scrollbar-evidence`, set in every Bash call that uses it.
  - `env -C /home/davey/projects/taiyaku/web ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`. Read the summary lines from the log, never a pipe through `tail`.
  - `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`, each under `env -C /home/davey/projects/taiyaku/web`.
  - Coverage: `./node_modules/.bin/vitest run --project node --project jsdom --coverage --reporter=default`, then from the repo root `node scripts/coverage-floor.mjs`. The browser project counts for nothing here, so `Chrome.tsx`'s and `Filmstrip.tsx`'s new code needs jsdom cases; each task names them. A file under its floor gets a test of the kept code; a floor is never lowered.
  - Whole projects only as a task's final check. `-t` is for watching one mutation fail, and for running a real-key case alone.
  - A task that touches CSS or a `*.browser.test.tsx` runs the whole browser project, which CI never runs.
  - **Task checks**, every task's last step: `tsc -b`, `biome check`, coverage and the floor, the whole browser project (both launches), node and jsdom whole, and the comment check.
- **Gate** (Finish only, and after any late code fix): `env -C /home/davey/projects/taiyaku -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/gate.log 2>&1; echo "exit $?" >| $E/gate.exit`. Run the repo's own script, never a copy, at load < 24 (`cat /proc/loadavg`). Quote the `all green` line.
- **Browser cases.**
  - A real mouse from `tests/support/mouse.ts`. Every case that measures a box runs in both reading directions. The cases the spec names run on a long volume and a short one.
  - Geometry is asserted only in the browser project; jsdom lays nothing out.
  - Each case first asserts what makes right and wrong differ (THE STATE THIS TEST NAMES, as the existing cases do): which bound decides the width, that the rail moved far enough, that a spacer would be seen.
  - A case that measures the line at rest moves no mouse before it measures. A case about sizes reads them after finishing the transitions (`for (const a of el.getAnimations()) a.finish()`), unless the transition is what it is about.
  - Timing cases wait by polling with a deadline and assert the elapsed time from `performance.now()` in both directions (no sooner than 1000 ms, and it does happen). A fixed sleep is never the assertion.
  - `pointerRulesAs` switches the stylesheets only; `matchMedia` still answers for this Chromium's fine pointer. So a case that names a device asserts what is drawn or pressed, not `data-awake`. Call it first: `onTestFinished(pointerRulesAs(…))`.
  - Real keys: focus an element in the page before the first real key. Each real-key case also passes alone with `-t`.
  - Never Chromium's touch emulation. `Input.dispatchTouchEvent` through `tests/support/touch.ts` is fine.
  - Chromium crashes: a red browser run with import or connection errors and no failed assertion is headless Chromium crashing. Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Watched mutations.** Every new or rewritten assertion is watched failing. Put the named defect in by hand, watch the test go red on its assertion (not on a throw from setup), restore it, and check the restore with `git diff`. Record each in the task report: the defect, the test, the message. A rewritten test is also watched failing against the prototype's predecessor behaviour where the step names one.
- **Pre-existing failures** count only if the same command fails at main fed04e28, checked in a detached scratch worktree under the session scratchpad. Copy `web/node_modules`, never symlink it, and copy `crates/taiyaku-wasm/pkg`. Report them; do not fix them.
- **Classes.** `classNames.test.ts` fails on a class no selector names; `.reader-progress-grip` has its rule.
- **Commits.** One per task, its subject naming O286. Never `--amend`. Before committing, check the previous task's hash is an ancestor of `HEAD`. Branch `o286-strip-scrollbar`, in the main checkout, no worktree. Each task's report goes to `$E/T<n>-report.md`, written by Bash (`cat >| … <<'EOF'`); the Write tool is refused under `.superpowers/`.
- **Servers and other work.** Never touch 127.0.0.1:4398 or :4399 (the controller's) or anything under `.claude/worktrees/`. A task that serves the app uses the recipe in the Finish, a port checked free with `ss -lptn`, its own scratch copy under the session scratchpad, and stops its server by PID, then checks the port again.
- **No Rust.** If a task finds it needs a server change, it stops and reports.
- **Not in this plan** (spec, Non-goals): a drawn scrollbar on the rail, touch or pen on the line, keys, Firefox's bigger step on its own bar, paging toward the press.

## Review Focus

1. **A press on the band where the strip shows the whole volume**: nothing moves, focus and the selection stay as they were, and the line holds nothing. Task 3, cases (g) and (j).
2. **A still hold after a jump into the unrendered middle of a long volume**: the rail comes to rest with tiles in view and the mark centred on the pointer, with no frame loop between the re-stand and the window's rebuild. Task 4, case (b).
3. **A hold let go outside the window**: the hold ends, a later move with no button scrolls nothing, and the line sleeps a second later. Task 3, case (i); Task 5, case (f).
4. **A line drag while correcting** (the strip shows the other edition): the rail moves, the mark follows, and no hover preview opens. Task 3, case (h).
5. **A touch laptop**: a mouse on the band drags. A finger's tap on the band moves and wakes nothing, and a finger's swipe on the tiles still scrolls the strip. Task 3, case (d); Task 5, case (m).

---

### Task 1: The baseline: the pointer table, one condition, the guard, and the tests the prototype made false (O286)

Spec: goals 3 and 6, "Left to the builder" (the rewritten tests); gaps 1–3.

**Files:**
- `web/tests/support/pointer.ts`: `HOLDS` gains `['(any-pointer:fine)', { phone: false, mouse: true, 'touch laptop': true }]`, and its doc names the line's band and wake beside the `i` rule.
- `web/src/styles/reader.css`: the band's block and the thickening block both under `@media (any-pointer: fine)`. Gap 3's 14-line block is cut to its why.
- `web/src/Chrome.tsx`: the wake effect's `matchMedia` query becomes `'(any-pointer: fine)'`; gap 3's two blocks are cut to their why.
- `web/tests/Chrome.test.tsx`, describe "the mark on the progress line" (~281-600).
- `web/tests/Chrome.browser.test.tsx`: `FLOOR_PX` and `SHARE` (~56-57) become `MARK_MIN_PX = 64` and `MARK_MAX_PX = 160`, and `widthDue` (~178) clamps between them. The file's doc says `clamp()` for `max()`, and the volume constants' docs say which bound each length meets.
- `web/tests/styles/reader.css.test.ts` (~286-311).
- Unchanged, as the spec says: the rail's own drag, wheel and glide cases (`Filmstrip.browser.test.tsx`, and Chrome.browser's "follows a real drag" and "follows a glide").

**Interfaces:**
- Produces: `MARK_MIN_PX`, `MARK_MAX_PX` and `widthDue(rail: HTMLElement, line: HTMLElement): number` in `Chrome.browser.test.tsx`. Task 2 moves them to `tests/support/readerBars.tsx`.
- Produces: the `HOLDS` key `(any-pointer:fine)`, which Tasks 3, 5 and 6 switch by.

- [ ] **Step 1: Baseline.** Run node, jsdom and browser at `HEAD` into logs, and list every red case in the report. Expect red: the jsdom cases that assert `max(32px, …)` and the inline insets, the reader.css case "lets the mark stand proud…" (it finds 4 rules, not 1), the Chrome browser width and row cases, and every case in the files that call `pointerRulesAs` or `coarseRulesOn` (a throw naming `(any-pointer: fine)`). Anything else red is checked at main fed04e28 before going on.
- [ ] **Step 2: One condition, and the table.**
  - Node, `reader.css.test.ts`: "asks one pointer condition of the progress line, `(any-pointer: fine)`". `atRulesOf` for `.reader-progress-grip` and for each thickening rule's prelude (`.reader-progress`, `.reader-progress-fill`, `.reader-progress-shown`): every at-rule that names pointer or hover is exactly `@media (any-pointer: fine)`.
  - jsdom, `Chrome.test.tsx`: "asks for the wake by `(any-pointer: fine)`". With `window.matchMedia` stubbed as a spy answering `{ matches: false }`, a visible mount calls it with `'(any-pointer: fine)'` and with no query that names hover.
  - Run: both fail. Then write the CSS condition, the query and `HOLDS`. Run node, jsdom and the whole browser project. The two pass, and the `pointerRulesAs` files are green again.
- [ ] **Step 3: Rewrite what the prototype made false.**
  - jsdom width strings: QUARTER is `clamp(64px, 25%, 160px)` and SIXTEENTH `clamp(64px, 6.25%, 160px)`, in every `left` and `width` the describe asserts. The 2001/2000 case matches `^calc\(0 \* \(100% - clamp\(64px, `. "is as wide as an eighth of the share shown, or the floor…" becomes "is as wide as the share shown, held between 64 and 160 px".
  - jsdom insets (~369-388): `[shown.style.top, shown.style.bottom]` is `['', '']`, because the insets are the stylesheet's.
  - Node (~286-311) becomes "no rule for the progress line, under any at-rule, declares overflow, clip-path or contain". It reads `atRulesOf(readerSource, '.reader-progress')`. Control: at least one rule at the top level and one inside `@media (any-pointer: fine)` are found. Each body declares none of the seven properties. Add "draws the mark one pixel proud of the line at rest": a top-level `.reader-progress-shown` declares `top: -1px` and `bottom: -1px`.
  - Browser, both directions, each fixture asserting its bound first (`share × line width` against 63 or 161):
    - (a) "is 64 px where the share on show is narrower, one pixel proud of the line at rest": a length that gives under 63, which `LONG` (120) no longer does at 1440 px. Width ≈ 64 (0 dp), line height 2, mark `[top, bottom]` is `[line.top − 1, line.bottom + 1]`.
    - (b) "is the share on show where that lies between 64 and 160 px".
    - (c) "is 160 px where the share on show is wider".
    - (d) "hides the mark … and draws it once it does not" (~611): drawn at `widthDue`.
    - (e) Late thumbnails (~652) and narrower window (~696): fixtures whose share lies between the bounds before and after, with each case's own `stale − due > 5` and share-change checks kept.
    - (f) "is drawn in the row above the line and the row below it" (~451) asks `document.elementsFromPoint(x, row)`. The mark is among the elements at `line.top − 1`, `line.top`, `line.top + 1` and `line.bottom`, and not at `line.top − 2` or `line.bottom + 1`. Its state line adds that `elementFromPoint` at the line's middle is `.reader-progress-grip`, which is why it asks for every element at a point.
  - Run node, jsdom and browser to green.
- [ ] **Step 4: Watched mutations.**
  - `markWidth` back to `max(32px, <share × 12.5>%)`: the jsdom width strings and browser (a)–(c) fail.
  - `max(64px, …)` with no upper bound: (c) fails. `min(…, 160px)` with no lower bound: (a) fails.
  - `top: -1, bottom: -1` back in `markStyle`: the jsdom insets case fails. The CSS insets removed: the node case and (a) fail.
  - `overflow: hidden` in the `(any-pointer: fine)` `.reader-progress` rule: the node case fails. At the top level: (f) fails.
  - The thickening block back on `(hover: hover) and (any-pointer: fine)`: the condition case fails. The prototype's query restored: the jsdom query case fails.
- [ ] **Step 5:** Task checks; the comment check is now green. Commit `Hold the progress line's mark width and pointer rules with tests (O286)`. Write `$E/T1-report.md` with Step 1's list.

---

### Task 2: Drag and jump (O286)

Spec: goal 1, Constraints, Approach 1, and "Left to the builder" (`placeMark`'s order); gap 5.

**Files:**
- `web/src/Chrome.tsx`: `scrollerRef`, `lineHeld`, `lineGeometry` and `scrollLineTo` are declared before `placeMark`. Its comments are rewritten where the tests show them false.
- `web/src/Filmstrip.tsx`: `StripScroller` and the `scroller` prop, held as they are.
- New `web/tests/support/readerBars.tsx`. It takes from `Chrome.browser.test.tsx`, which imports it: `volume`, `props`, `reader`, `inReader`, `railOf`, `lineOf`, `markOf`, `markBox`, `centre`, `frame`, `firstReportPast`, `untilAtRest`, `travel`, `laterSide`, `farEnd`, `scrollRail`, `MARK_MIN_PX`, `MARK_MAX_PX` and `widthDue`.
- New `web/tests/Chrome.drag.browser.test.tsx`.
- `web/tests/Chrome.test.tsx`: new describe "the progress line under a mouse (O286)".
- `web/tests/Filmstrip.test.tsx`: new describe "the strip's scroller (O286)".

**Interfaces:**
- Consumes: Task 1's `MARK_MIN_PX`, `MARK_MAX_PX` and `widthDue`.
- Produces, in `readerBars.tsx`, beside the moved helpers:
  - `gripOf(host: HTMLElement): HTMLElement`
  - `lineRow(host: HTMLElement): number`: a whole-pixel `y` on the line.
  - `atAlong(host: HTMLElement, d: number): number`: the window `x` that lies `d` px from the line's start as the reader sees it (its left edge, or its right where the line is mirrored).
  - `markFrom(host: HTMLElement): { start: number; width: number }`: the mark's drawn start, measured the same way, and its width.
  - Tasks 3–6 import all of these.

- [ ] **Step 1: Move the helpers** with no change to any case, and run the browser project's Chrome files to green.
- [ ] **Step 2: Cases.** Browser, `for reading of ['ltr', 'rtl']`, and where marked over `[['long', LONG], ['short', SHORT]]`. Each short volume still overflows, which the case asserts.
  - (a) "keeps the mark under the pointer where it was taken, through a drag of the mark, ${kind} ${reading}" (both lengths). Press at 30 % into the mark on `lineRow`, then 10 moves of 20 px toward the end with more room. Two frames after each move, `markFrom().start + 0.3 × width` is within 0.5 px of the pointer's distance from the line's start. State: the rail has moved, and the move would show a mark left behind.
  - (b) "centres the mark on a press of the line away from it, and goes on as a drag, ${kind} ${reading}" (both lengths). With the mark at the start, press at 75 % along. Two frames on, the mark's centre is within 0.5 px of the pointer, and it stays so through three more moves of 20 px.
  - (c) "stops at either end of the volume, ${kind} ${reading}" (both lengths). Drag 100 px past the line's start: `Math.abs(rail.scrollLeft) ≤ 0.5` and the mark is flush with the start (0.5 px). Then 100 px past the end: `Math.abs(rail.scrollLeft)` is `travel(rail)` within 1, and the mark is flush with the end.
  - (d) "carries nothing on after the release, with the reader's animation on, ${reading}". `motion: { animate: true, ms: 80 }`. Press 3 px above the line's top, which is in the band, over the rail's padding. `mouse.clock` steps 15 px per 15 ms for 8 moves, and the release comes with the last. After `untilAtRest`, `scrollLeft` equals its value read just after the release (±1). Control: the same flick on a tile glides the rail more than 100 px past its release.
  - (e) "stops a glide the rail is running, ${reading}". Flick a tile as Chrome.browser's glide case does. State: two reads a frame apart differ. Press the mark's middle and hold: after `untilAtRest`, `scrollLeft` is within 1 px of its value two frames after the press.
  - (f) "moves the rail with no commit of the bars or the strip, ${reading}". The bars sit in React's `Profiler`, as `Chrome.test.tsx`'s "moves the mark for a scroll with no render" has them. A drag of 5 moves of 10 px that brings no spacer near: state, the first rendered `data-group` is unchanged. Commits are unchanged. Control: a rerender commits.
  - jsdom, "the progress line under a mouse (O286)". Stub `getBoundingClientRect` on the line (0–1000 wide) and the mark (64 wide), and the rail's widths (`scrollWidth` 1600, `clientWidth` 400). `setPointerCapture`, `hasPointerCapture` and `releasePointerCapture` are stubbed as in the glide describe (~719).
    - (g) "scrolls the rail to where a press on the mark and a move put it, both ways". A press 19.2 px into the mark and a move of 93.6 px give `along` 0.1, so `scrollLeft` is 120, or −120 for `rtl`.
    - (h) "takes no press but a mouse's primary button". Pen, touch, button 1 and button 2 leave `scrollLeft` alone and set no `data-held`.
    - (i) "ends the hold on a move with no button down, on a release, on a cancel and on lost capture". After each, a further move moves nothing and `data-held` is gone.
    - (j) "prevents the mousedown that follows a held press, and no other": `defaultPrevented` on a `mousedown` dispatched while held, and not without a hold.
  - jsdom, `Filmstrip.test.tsx`, "the strip's scroller (O286)":
    - (k) "stands the rail along its travel from the widths it has when asked, both ways": `scrollAlong(0.25)` at 1600/400 gives 300 (−300 `rtl`); after the widths change to 2400/400, it gives 500.
    - (l) "empties the ref when the strip goes, and fills it again for a new reading direction".
  - Run: all pass against the prototype, which is the behaviour under test. Step 4 shows each.
- [ ] **Step 3: Order.** Declare the four before `placeMark`, with no other change. Check with `grep -n` that each declaration's line comes before `const placeMark`, and run jsdom and the Chrome browser files to green.
- [ ] **Step 4: Watched mutations.**
  - `grab` always 0.5: (a) fails. A press off the mark takes `grab` 0: (b) fails.
  - `scrollAlong` without `later`: (a)–(c) `rtl` and (k) fail. `scrollAlong` reckons the travel as `rail.scrollWidth`: (a) and (k) fail.
  - `scrollLineTo`'s clamp narrowed to `[0.02, 0.98]`: (c) fails.
  - `scrollAlong` without `stopGlide.current()`: (e) fails.
  - `.reader-progress-grip { pointer-events: none }`: (d) fails, since the press falls to the rail, which drags the other way and glides.
  - A `setPreview(undefined)` in `onLineMove`: (f) fails.
  - Drop the `pointerType`/`button` check: (h) fails. Drop the `(event.buttons & 1) === 0` end: (i)'s first part fails.
  - `onLineMouseDown` prevents always: (j) fails.
  - Drop the cleanup that nulls the ref: (l) fails.
- [ ] **Step 5:** Task checks, reading `Chrome.tsx`'s and `Filmstrip.tsx`'s coverage by name. Commit `Hold the progress line's drag and jump with tests in both directions (O286)`. Write `$E/T2-report.md`.

---

### Task 3: The press band, and what a press leaves alone (O286)

Spec: goals 2 and 6, Anti-goals (first line), Non-goals (keys); gap 4.

**Files:**
- `web/src/Chrome.tsx`: `onLineDown` prevents a primary mouse press before it asks whether there is a mark to take (gap 4).
- `web/tests/support/mouse.ts`: `pen`, with `moveTo`, `press` and `release` as `mouse` has them, sent with `pointerType: 'pen'`, sharing its pointer state and its `afterEach`.
- `web/tests/support/touch.ts`: `swipeAlong(x: number, y: number, from: number, to: number): Promise<void>`. A touch pressed at `x + from`, moved in 15 px steps a frame apart and lifted at `x + to`, sent outside `act` as `tapAt` is.
- New `web/tests/Chrome.press.browser.test.tsx`.
- `web/tests/Chrome.test.tsx`: case (j).

**Interfaces:**
- Consumes: Task 2's `readerBars.tsx`; `pressArea`, `near` and `pointerRulesAs` from `support/pointer.ts`; `tapAt` from `support/touch.ts`.
- Produces: `pen` and `swipeAlong`, which Task 5 uses.

- [ ] **Step 1: Cases.** Browser; both directions unless the case says otherwise.
  - (a) "takes a press only in a 10 px band centred on the line, across its width". Under `pointerRulesAs('mouse')`, `pressArea(gripOf(host))` matches (`near`) height 10, top `line.top − 4`, bottom `line.bottom + 4`, and the line's left and right.
  - (b) "leaves a tile's press above the band and the readout's text below it as they were".
    - The lowest tile's `pressArea` bottom is no lower than the band's top, and the readout text's top is no higher than the band's bottom.
    - A real click 12 px above the line's middle calls `onNavigate` with that tile's slot. Control: the same click with the grip at `display: none` calls the same.
    - A real press 12 px below the line's middle, in the keep zone and under the band, then a 60 px move: the rail does not move and no `data-held` is set.
  - (c) "draws no band on a phone". Under `pointerRulesAs('phone')`, `gripOf(host).getClientRects()` is empty, and `elementFromPoint` 3 px above the line lies inside the rail.
  - (d) "on a touch laptop, a mouse drags the line and a finger does not". Under `pointerRulesAs('touch laptop')`, the band is drawn. `tapAt` on the mark leaves `scrollLeft` and `data-held` alone. `swipeAlong` across the tiles moves the rail, which is the control that the finger works on the strip. A mouse press on the mark and a 40 px move move the rail.
  - (e) "takes no pen press": `pen` presses the mark and moves 40 px; `scrollLeft` is unchanged and no `data-held` is set.
  - (f) "turns no page, opens no tile, selects no text, and leaves focus where it was". `it.each` over focus on the current tile, on a button outside the bars (`elsewhere`), and on nothing (`document.body`).
    - Press the mark, drag 60 px and release; then press the line off the mark and release.
    - Each time, `document.activeElement` is unchanged, `getSelection()?.toString()` is `''`, and `onNavigate` was not called.
    - With focus on the tile, a real Space then calls `onNavigate` once, with that tile's slot.
    - The line keeps `aria-hidden="true"`, and neither it, the band nor the mark has a `tabIndex`.
  - (g) "moves nothing and leaves focus where it was on a press of the band where the strip shows the whole volume" (`FITS`; gap 4). Focus on `elsewhere`; press the band's middle, move 40 px and release. Focus and the empty selection are kept, no `data-held` is set, and `scrollLeft` is 0.
  - (h) "drags the line while correcting, and opens no preview" (`correcting={{ to: 'ed2' }}`). The drag moves the rail and the mark follows. No `.reader-preview-img` is rendered at any step.
  - (i) "ends a hold let go outside the window". Press the mark, move out of the test frame, release there, and come back with no button. Moves then scroll nothing, and `data-held` is gone.
  - jsdom (j): "prevents a mouse press on the band where the mark is hidden, and leaves a pen's alone". With the WHOLE rail, a mouse `pointerdown` with button 0 is `defaultPrevented` and sets no `data-held`; a pen's is not prevented.
  - Run: (g) and (j) fail (gap 4); the rest pass.
- [ ] **Step 2: Implement** gap 4 and run the whole browser project and jsdom to green.
- [ ] **Step 3: Watched mutations.**
  - Grip insets of ±6 px: (a) fails. Insets of ±16 px, so that the keep zone takes presses: (b)'s press below fails.
  - The grip's `display: block` outside the media rule: (c) fails.
  - `onLineDown` accepts `touch`: (d) fails. It accepts `pen`: (e) fails.
  - Drop both `preventDefault`s: (f) fails. Restore the prototype's return before `preventDefault`: (g) and (j) fail.
  - `onLineDown` returns while `filmstrip.correcting`: (h) fails.
  - (i): find the defect it catches by removing `onPointerUp`, then the buttons check. If neither turns it red (Chromium delivers the captured release either way), say so in the report and keep it as Review Focus 3's pin.
- [ ] **Step 4:** Task checks. Commit `Hold the progress line's press band, and keep a press on it from moving focus where there is nothing to drag (O286)`. Write `$E/T3-report.md`.

---

### Task 4: A hold while the rail's length changes; the long drag; the 4.4 px sample (O286)

Spec: Constraints (the rail's `scrollWidth` changes while held; the plan measures the rebuilds), "Left to the builder" (the 4.4 px sample); gap 7.

**Files:**
- `web/src/Chrome.tsx`: `placeMark`, only if Step 3 finds a painted frame off the pointer.
- `web/tests/Chrome.drag.browser.test.tsx`: new describe "a hold while the rail's length changes".
- Evidence: `$E/T4-long-drag.log`, `$E/T4-chase.txt`, and `$E/check5-rerun.mjs`, a copy of the prototype's `check5.mjs` reduced to its on-mark drag, pointed at this task's port, and reading the error at each of 8 frames after each move with the rail's `scrollWidth`.

**Interfaces:**
- Consumes: Task 2's `readerBars.tsx`. A late-thumbnail loader is lifted from `Chrome.browser.test.tsx`'s "after thumbnails load late" case (~652) into `readerBars.tsx` as `loadThumbnailsLate(rail: HTMLElement): Promise<void>`, and that case uses it.

- [ ] **Step 1: Cases**, both directions.
  - (a) "keeps a still-held mark under the pointer in every frame as thumbnails load late", on Task 1's late-thumbnails volume. Press 30 % into the mark and hold. Start a `requestAnimationFrame` loop that, each frame, records the grab point's distance from the pointer. Then `loadThumbnailsLate`, then `untilAtRest`.
    - State: `rail.scrollWidth` grew by enough that a mark placed by the new share alone would stand more than 2 px off. Compute it from the widths before and after.
    - Every sample is ≤ 1 px, and the last is ≤ 0.5.
  - (b) "comes to rest with tiles in view and the mark under the pointer when held still after a jump into a long volume's middle". A 400-page volume at `currentIndex` 0. Press the line 60 % along and hold still.
    - `untilAtRest` returns with the button down.
    - No `[data-testid="strip-spacer"]` intersects the rail's visible box.
    - The mark's centre is within 0.5 px of the pointer.
  - (c) "reaches the end of a volume held past it while its length grows". Press the mark, move past the end, then `loadThumbnailsLate`. After rest, `Math.abs(scrollLeft)` is `travel(rail)` within 1.
  - Run.
- [ ] **Step 2: Measure the long drag** (spec Constraints: measured, not assumed). Use a scratch copy of (b)'s mount that logs with `console.log`, run with `--reporter=default` into `$E/T4-long-drag.log`, and never commit it. Drag the mark from the line's start to its end in 15 px steps, with a `Profiler` around the bars. Per step, log the commits since the last step, whether a spacer intersects the rail's visible box two and three frames after the move, and the grab error.
  - Commit the case "drags a long volume's mark from end to end with tiles in view as each move settles": no step leaves a spacer in view three frames after its move, and the rail ends at its far end with the mark flush.
  - If a step does leave one, stop and report the figures to the controller. A wider window for a line drag is the owner's decision, not this task's.
- [ ] **Step 3: Chase the 4.4 px sample** (gap 7), with the findings in `$E/T4-chase.txt`.
  1. In the browser project: run (a) ten times alone with `-t`, under `cdp().send('Emulation.setCPUThrottlingRate', { rate: 4 })` in a scratch copy of the case. Note any frame over 1 px.
  2. On the built app: serve a scratch copy of the test library as the Finish's drive does (built app, release binary, a free port). Run `$E/check5-rerun.mjs` 20 times per direction, at the load found and under a rate of 4. Record the per-frame maxima, and whether each sample over 1 px comes with a change of `scrollWidth`. Stop the server by PID and check the port.
  3. Decide.
     - A painted frame (sampled in `requestAnimationFrame`) with the mark more than 1 px off the grab point during a hold is a defect against the Anti-goal "the mark lagging the pointer". Fix `placeMark` so that, while held, the mark is written from the `along` the re-stand asks for and not from the share before it, in the same write. (a) must then hold every frame without throttling. Rerun step 2's script to show the sample gone.
     - If no painted frame is off, and only reads taken between a write and the next frame were, record that and the figures, and change nothing.
- [ ] **Step 4: Watched mutations.**
  - Delete `placeMark`'s re-stand (`if (held) scrollLineTo(…)`): (a) and (c) fail.
  - `scrollAlong` reckons the travel once per hold, at the press: (c) fails.
  - If Step 3 made a fix, revert it: (a) fails.
- [ ] **Step 5:** Task checks. Delete the scratch copy in its own Bash call by its literal path, after the library's digest and marker check. Commit `A hold on the progress line follows the rail's changing length (O286)`. Write `$E/T4-report.md` with Step 2's and Step 3's figures, for Task 7.

---

### Task 5: Wake and sleep, and what the bars' going leaves behind (O286)

Spec: goal 4, goal 2 (no touch or pen), Anti-goals (the bars hiding, listeners); gap 6.

**Files:**
- `web/src/Chrome.tsx`: an effect on `[visible]` whose cleanup ends a hold as `onLineEnd` does: `lineHeld` cleared, `data-held` gone, the capture released if it is still held (gap 6). `wakeOnMouse`'s comments are rewritten where the tests show them false.
- New `web/tests/Chrome.wake.browser.test.tsx`.
- `web/tests/Chrome.test.tsx`: new describe "the line wakes for a mouse (O286)".

**Interfaces:**
- Consumes: `readerBars.tsx`, plus `pen` and `swipeAlong` from Task 3.

- [ ] **Step 1: jsdom cases.**
  - Setup:
    - `vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'requestAnimationFrame', 'cancelAnimationFrame', 'performance'] })`.
    - `window.matchMedia` answers `matches: true` only for `'(any-pointer: fine)'`.
    - The line's rect is 2 px tall at y 600; the bar's is 0–1440 wide.
    - Events are `PointerEvent`s dispatched on `window`, or on `document` for `pointerout`.
  - (a) "wakes a frame after a mouse moves anywhere, and sleeps 1000 ms after the last move". After a move at (700, 300), `data-awake` is there one frame on, still there at +999 ms, and gone at +1000 ms. A second move at +500 keeps it until +1499, and it is gone at +1500.
  - (b) "counts a release as a move".
  - (c) "wakes for no touch and no pen".
  - (d) "stays awake while the pointer rests within 16 px of the line, across the whole bar".
    - At (5, line bottom + 16) it is still there at +5000; at (5, line bottom + 17) it is gone at +1000; at (5, line top − 16) it is still there.
    - A move out of the zone then sleeps it 1000 ms later.
  - (e) "stays awake through a hold, and sleeps 1000 ms after its release".
  - (f) "sleeps once the pointer leaves the window, wherever it last was". In the zone, a `pointerout` with `relatedTarget: null` puts it to sleep 1000 ms later.
  - (g) "leaves nothing behind when the bars hide, or when the reader goes". Spy on `window` and `document`'s `addEventListener` and `removeEventListener`. Mount and move, so that a frame and a timer are pending. After `rerender` with `visible: false`, every listener added is removed with the same type, listener and capture, and `vi.getTimerCount()` is 0. The same holds for an unmount.
  - (h) "ends a hold when the bars hide, and the line comes back at rest" (gap 6). Press, rerender `visible: false`, then `true`.
    - The new line has neither `data-held` nor `data-awake`.
    - A move wakes it, and 1000 ms later it sleeps.
    - A `buttons: 1` move on it moves no rail.
  - (i) "neither listens nor wakes where the device has no fine pointer": with `matchMedia` false, no window listener is added, and a move sets nothing.
  - Run: (h) fails (gap 6); the rest pass.
- [ ] **Step 2: Browser cases**, real input.
  - (j) "wakes on a real mouse move over the page, and sleeps a second after". State: no `data-awake` before the move. Move to a point above the bars. `data-awake` is there within two frames. Poll until it goes, with a 3 s deadline; the time since the move is ≥ 1000 ms.
  - (k) "stays awake while the mouse rests 12 px below the line near the bar's edge, and sleeps after it moves away". `x = bar.left + 4`, outside the line's side margin. It is still awake at 2000 ms. After a move to 40 px below the line, it sleeps no sooner than 1000 ms later.
  - (l) "stays awake through a still hold of 2 s, and sleeps a second after the release".
  - (m) "wakes for no finger and no pen": `swipeAlong` over the page area and `pen` moves, then two frames and 100 ms; no `data-awake`.
  - (n) "ends a hold the bars' hiding cuts off, ${reading}".
    - Press the mark and drag 40 px.
    - `rerender(reader({ …, visible: false }))`, which is what Escape does through `App`.
    - Move 60 px with the button down, release, then `rerender` with `visible: true`.
    - The new line has no `data-held`. The new rail's `scrollLeft` does not change across a further 60 px move, and the line sleeps a second after it.
  - Run: (n) fails until Step 3.
- [ ] **Step 3: Implement** gap 6. Run jsdom and the whole browser project to green.
- [ ] **Step 4: Watched mutations.**
  - `LINE_WAKE_IDLE_MS` set to 2000: (a) and (j) fail.
  - `LINE_KEEP_PX` set to 0: (d) and (k) fail. The zone's `x` taken from the line's box: (d)'s `x = 5` cases and (k) fail.
  - `held` as `() => false`: (e) and (l) fail.
  - Drop the `pointerType` check: (c) and (m) fail. Drop the `pointerup` listener: (b) fails. Drop `onOut`: (f) fails.
  - A cleanup that leaves `pointerup` attached: (g) fails.
  - Remove gap 6's effect: (h) and (n) fail. Remove the `matchMedia` gate: (i) fails.
- [ ] **Step 5:** Task checks. Commit `Hold the progress line's wake and sleep, and end a hold the bars' hiding cuts off (O286)`. Write `$E/T5-report.md`.

---

### Task 6: How the line is drawn and eased, and the Title field's failure (O286)

Spec: goals 5, 7, 8 and 9, Anti-goals (the line, mark and fill in step; no layout shift), "Left to the builder" (the built app).

**Files:**
- `web/src/styles/reader.css` and `web/src/styles/library.css`: only as the cases require. The reader rules' comments are rewritten where the tests show them false.
- New `web/tests/support/drawn.ts`: `drawn(el)`, `pixelAt(image, x, y)` and `apart(one, other)`, moved from `Config.browser.test.tsx` (~5091-5115), which imports them.
- New `web/tests/Chrome.look.browser.test.tsx`.
- `web/tests/styles/reader.css.test.ts`, `web/tests/styles/library.css.test.ts`, `web/tests/builtStylesheet.test.ts`, and `web/tests/MetadataEditor.browser.test.tsx` ("puts a failure 8 px under the field…", ~604).

**Interfaces:**
- Consumes: `readerBars.tsx`; `tokenColour` and `channels` from `support/colour.ts`.

- [ ] **Step 1: Cases.**
  - Node:
    - (a) "registers one number, `--grow`, that the line, its fill and the mark are drawn from". The `@property --grow` body declares the three descriptors (Global Constraints). Inside `@media (any-pointer: fine)`, the line's `box-shadow`, the fill's `top`, `bottom` and `border-radius`, and the mark's `top`, `bottom` and `border-radius` each name `var(--grow)`.
    - (b) "eases a wake over 150 ms out and a sleep over 300 ms in and out, and none when still or reduced". Compared with whitespace removed:
      - `--grow300msease-in-out` on `.reader-progress`;
      - `--grow150msease-out` on `.reader-progress:where([data-awake],[data-held])`;
      - `none` on `.reader-progress:where([data-still])`, later in the same block;
      - `none` on `.reader-progress` under `@media (prefers-reduced-motion: reduce)`, a block that names no pointer and no hover.
    - (c) In `builtStylesheet.test.ts`: "keeps the line's registered number and its pointer block in the build". The built text has `@property --grow` with its three descriptors, and a `.reader-progress` rule with `transition` under the built form of `(any-pointer: fine)`, compared without whitespace.
    - (d) In `library.css.test.ts`: "draws the Title form's failure in --danger". The `.metadata-editor .failure` rule's `color` is `var(--danger)`.
  - Browser, both directions:
    - (e) "is 2 px with a 2 px fill and a 4 px mark at rest, and 4, 4 and 8 awake, all about the line's centre". Awake comes from a real mouse move, with the transitions finished.
      - The line's rect stays 2 px tall. Its computed `box-shadow` has offsets `0px -1px` and `0px 1px` in `--line`'s colour awake, and 0 at rest.
      - The fill is 2 px tall at rest and 4 awake (`line.top − 1`); the mark is 4 at rest and 8 awake (`line.top − 3` to `line.bottom + 3`).
    - (f) "moves no other box as it wakes or sleeps". Every element under `.reader-bottom-bar` and the top bar but the fill and the mark has the same `toJSON()` rect asleep, mid-wake (the transition paused at 75 ms), awake, and asleep again.
    - (g) "grows the line, the fill and the mark as one". With animation on, wake the line by a real move. Pause its `--grow` transition from `line.getAnimations()` and seek it to 0, 37, 75, 112 and 150 ms.
      - At 37–112 ms, `--grow` read from the line lies strictly between 0 and 1.
      - At every point, `(fill height − 2) / 2` and `(mark height − 4) / 4` equal it within 0.02.
      - The transition's timing is 150 ms `ease-out`; after the sleep starts, 300 ms `ease-in-out`.
    - (h) "grows at once with the reader's animation off, and where the system asks for reduced motion". With `motion.animate: false`, after the move's frame `line.getAnimations()` is empty and the mark is 8 px. The same holds with animation on and `prefers-reduced-motion: reduce` emulated (`Emulation.setEmulatedMedia`, reset in `onTestFinished`).
    - (i) "rounds the fill's end at the reader's place and squares its start", at progress 0.5.
      - Computed, at rest and awake: the fill's right two corner radii are half its height (`1px`, then `2px`), and its left two are `0px`. In `ltr` the fill's left is the line's left; in `rtl` its right is the line's right.
      - As drawn, awake, in a `.reader` 300 px wide at `zoom: 4`: in `drawn(line)`, the column one device pixel inside the fill's leading end has top and bottom fill rows more than 60 `apart` from `--accent`, while its middle rows are the accent. The column one pixel inside its start is the accent in every fill row. In `rtl` the leading end is the left one.
    - (j) "draws nothing at 0 % and stays inside the line at 100 %": at progress 0 the fill's rect is 0 wide, asleep and awake. At progress 1 it lies within the line's, to 0.05 px, asleep and awake.
    - (k) In `MetadataEditor.browser.test.tsx`, "puts a failure 8 px under the field…" also asserts that the failure's computed `color` is `tokenColour(holder, '--danger')`.
  - Run: all pass against the prototype except anything Step 2 records.
- [ ] **Step 2: Fix** what a case shows wrong. Run node, jsdom and the whole browser project to green.
- [ ] **Step 3: Watched mutations.**
  - The awake rule's `--grow: 2`: (e) fails. The awake line as `height: 4px` in place of the shadows: (e)'s rect and (f) fail.
  - The fill drawn from a `[data-awake]` rule at a fixed −1 px in place of `var(--grow)`: (a) and (g) fail.
  - 150 and 300 swapped: (b) and (g) fail. Drop the `[data-still]` rule: (b) and (h)'s first half fail. Drop the reduced-motion block: (b) and (h)'s second half fail.
  - Drop `@property`: (a), (c) and (g) fail, since (g) then has no value in between.
  - The radius on the left corners: (i) fails in both directions. On all four: (i)'s square start fails.
  - The fill at `left: -1px`: (j) at 100 % fails.
  - Drop the `.failure` colour: (d) and (k) fail.
- [ ] **Step 4:** Task checks. Commit `Hold how the progress line is drawn and eased, and colour the Title field's failure (O286)`. Write `$E/T6-report.md`.

---

### Task 7: The ledger, the notes, and O216's and O138's records (O286)

Spec: "Left to the builder" (Docs). The close date is the day this task runs (`date +%F`). Every figure is measured or taken from an evidence file named beside it, never added to an old one.

- [ ] **Step 1: Close O286** with the five-part edit in `docs/status.md`:
  1. The row's checkbox (~1911) is `[x]`.
  2. The row's text begins `**Closed <date> by the strip's progress line as a scrollbar: …**` and says what was done, by symbol:
     - `Chrome.tsx`'s `onLineDown`, `onLineMove`, `onLineEnd` and `wakeOnMouse`;
     - `Filmstrip.tsx`'s `StripScroller`;
     - `styles/reader.css`'s `.reader-progress-grip` and `--grow`.
     It covers drag and jump for a mouse only, the 64–160 px mark, the band, and the wake. The Where column names the same symbols beside `shownShare`.
  3. Bullets:
     - O286's bullet in "The compact series tiles of 2026-11-14" (~4931) becomes "… Closed." and gains one sentence pointing at the new section.
     - After O287's bullet and before "## What comes next" (~4954), add "**The strip's progress line as a scrollbar of <date>** (`docs/superpowers/specs/2026-11-14-strip-scrollbar-design.md`) closed O286", then O286's bullet. It says what the line now does (goals 1–9, briefly), what it replaced (O216's width rule; O216's spec §3, which left drag and jump out), what it gives back (O138's cost), and the gaps this plan settled (1, 4 and 6, and 7's outcome).
  4. The test counts at ~183. Run node, jsdom and the whole browser project with `--reporter=default`, and set the four web figures from their summary lines: `node`, `jsdom`, their total, and the browser project's. The Rust figure does not move.
  5. The "One hundred and ninety-six are now closed — … O283 and O284" sentence (~1498) gains O286 in order and one in its count. "What remains: the ninety-one open items" (~5475) loses one; its count word wraps, so grep the opener and read the next line.
- [ ] **Step 2: O216 and O138.** Each pointer is one italic line, and the prose is kept.
  - O216's row (~1841): `*Its width rule, an eighth of the share on show and never under 32 px, was replaced on 2026-11-14 by the owner (O286): the share, between 64 and 160 px.*`
  - Its notes section, "## O216: the strip under a hand, its mark and its drag (2026-10-12)", gets the same at its head.
  - Its spec, `2026-10-12-strip-position-and-drag-design.md`, gets a line beginning `*Replaced 2026-11-14 by the owner (O286):*` that names the new spec, under §2.1 "How wide" and "How tall" and under §3's first bullet.
  - O138's row (~1763): `*What this ruling cost, a mouse's fast way across a long volume, is given back on the progress line by O286; the rail still draws no scrollbar.*`
- [ ] **Step 3: Notes.** Add `## O286: the strip's progress line as a scrollbar (<date>)` at the end of `docs/notes.md`:
  - the six rounds, pointing at the spec's Goals rather than repeating them;
  - Task 4's figures: the long drag's commits and spacer frames, and the chase's finding;
  - the comment text cut in Task 1, where it holds a reason the docs lack.
  The Finish adds the drive's readings. Each figure names its evidence file.
- [ ] **Step 4: Sweep.**
  - `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`, checked against the three sentences.
  - Every table row holds exactly six `|`.
  - `rg -n 'O286|an eighth|32 px|fast way across|SHOWN_MARK|FLOOR_PX' docs web/src web/tests`: read every hit. Prose that speaks of the old rule or of O286 as open is corrected, or marked if it is history.
- [ ] **Step 5:** Commit `Close O286 in status.md, and point O216 and O138 at it`. Write `$E/T7-report.md`.

---

### Finish

1. **Whole-branch review** of `main..HEAD`, in two lanes: (a) behaviour, Tasks 2–5 (`Chrome.tsx`, `Filmstrip.tsx` and their tests); (b) the look, CSS, test support and docs, Tasks 1, 6 and 7. Each reviews against the spec, the gaps above and the Review Focus. Fix rounds go back to the same implementer by SendMessage. After a fix, grep for the finding's shape, and review the fixes too. If a fix adds tests after Task 7, measure the counts again.
2. **Gate** on the final commit, by the Gate line. Read `$E/gate.exit`.
3. **The drive**, on the built app.
   - **Setup** (the UI-prototype recipe):
     - `sha256sum` and `stat` the library, and a marker, before and after;
     - a scratch copy of `/home/davey/temp/manga-test/library.db` under the session scratchpad;
     - a release binary of this branch copied beside it, with the `local`/`pw` account;
     - `vite build` into the scratch dir;
     - served on a free port that is neither 4398 nor 4399.
   - **Browsers:** Playwright's Chromium and Firefox (`firefox.launch()` from `web/node_modules/playwright`; record the Firefox version, which must be at least 128 for the easing).
   - **What is driven:** at 1280×800, on the 237-page volume the prototype used and a short one, in both reading directions:
     - drag, jump, both ends, and no glide after the release;
     - wake, sleep, the keep zone and a hold;
     - a real Escape mid-hold;
     - presses 12 px above and below the line;
     - focus and Space after a press;
     - the round end at rest and awake (screenshot);
     - no layout shift.
     In Firefox, also check that the wake eases, with `--grow` between 0 and 1 mid-transition.
   - **Record and clean up:** readings go into `$E/drive-<browser>.txt` and the notes section. Stop the server by PID and check the port. Delete the scratch copy in its own Bash call by its literal path.
4. **Claim check** of the status edits, the notes, the pointers and the PR body. Put the PR body in pass 1; scope later passes to the correction commit.
5. **The PR body** at `$E/pr-body.md`, its title the first line, measured under 255 characters. It names the gaps above. Ask the owner before pushing, and offer to serve the build again for a hand check in their own Firefox first.

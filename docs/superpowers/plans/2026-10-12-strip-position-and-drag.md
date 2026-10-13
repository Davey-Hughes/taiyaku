# The strip's position mark, and dragging the strip: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close O216. The progress line under the reader's strip carries a
mark for where the strip stands in the volume, and a mouse can drag the
strip.

**Architecture:** Two tasks, one after the other, since both edit
`Filmstrip.tsx` and its tests. The production code of each was written as a
prototype and tried by the user in Firefox on 2026-10-12, and is on the
branch before its task starts. A task's work is to hold that code with
tests, each watched failing against its defect, and to make the few changes
the spec asks beyond the prototype. The controller then writes the documents
and runs the Finishing list.

**Tech Stack:** React 19, vitest (projects `node`, `jsdom`, `browser`),
biome, pnpm.

**Spec:** `docs/superpowers/specs/2026-10-12-strip-position-and-drag-design.md`,
approved by the user on 2026-10-12 as `089fa5b3`.

## Why the code came before its tests

The user asked to see the mark before choosing it, and chose its width over
four rounds on a running prototype. What they approved is the code on the
branch, so it is the starting point and not something to write again from a
description. The discipline that replaces writing the test first is the one
this project already keeps: **each test is watched failing with its defect
put back by hand**. A test that cannot be made to fail holds nothing.

## Global Constraints

- **Branch:** `strip-position-and-drag`, cut from `main` at `1eb5723e`. A
  task works in a worktree of its own, reset to the branch's head before its
  first step (`git reset --hard <head>`, then `git log -1` to prove it), and
  commits there. The controller cherry-picks.
- **A worktree lacks the gitignored wasm package `web/` links to.** First
  step: `cp -r /home/davey/projects/taiyaku/crates/taiyaku-wasm/pkg crates/taiyaku-wasm/pkg`,
  then `pnpm -C web install --frozen-lockfile --offline`.
- **No attribution lines anywhere**: not in commit messages, code comments
  or documentation.
- **Never `cd`**: this shell's `cd` is zoxide and has landed in another
  repository. Run every command from the worktree's root, web commands as
  `pnpm -C web …`. The Bash tool runs zsh with `noclobber`: write scratch
  output to a new file name or use `>|`. Quote globs. Never name a shell
  variable `path`. `grep` is a shell function here that skips logs it takes
  for binary: use `rg`, or `command grep -a`.
- **Every new test is watched failing with its defect put back by hand.**
  Copy the file aside (`cp X "$SCRATCH/X.orig"`), put the defect in, run the
  test, restore with `cp` and prove it with `cmp`. Never `git add -A`, never
  `git stash`. Put each failure's first line in the task's report. If the
  named defect does **not** make the test fail, stop and report: do not
  weaken the defect or change the test to make it fail. Keep the mutations
  and their output in a file beside the report.
- **A task is done only when whole projects pass**: `pnpm -C web run lint`,
  `pnpm -C web run typecheck`,
  `pnpm -C web exec vitest run --project node --project jsdom --reporter=default`,
  `pnpm -C web run test:browser`, and `pnpm -C web run test:coverage` with
  `node scripts/coverage-floor.mjs` as `scripts/check-all.sh` calls it.
  Under an agent vitest picks a reporter that hides every passing test's
  console output; `--reporter=default` shows it.
- **Component tests use this repository's own helpers**
  (`web/src/testing/mount.ts`: `mount`, `rerender`, with `act`). There is no
  `@testing-library/react` here.
- **A real-input browser test focuses the page first**, since a key, a wheel
  or a mouse press sent before anything in the test frame has focus goes to
  the frame. Run each new real-input case alone with `-t` as well as in the
  project. `Filmstrip.browser.test.tsx`'s wheel cases show the pattern.
- **Both reading directions, every case that scrolls.** The rail is
  `row-reverse` for a work read right to left and its `scrollLeft` runs
  negative there. A fixture in one direction passes a handler that forgot
  the other.
- **A class a component writes has a stylesheet rule in the same commit**
  (`web/src/classNames.test.ts`), and a colour is a token
  (`web/src/colourLiterals.test.ts`).
- **A failure is "pre-existing" only if it fails the same way on `main` at
  `1eb5723e`**, checked in a worktree, never against another commit on this
  branch.
- **No test may need a wall-clock race to pass.** A race may make a test
  fail under its defect; it may never be what makes it pass.
- **No figure in a code comment that a later change would have to correct.**
- **Comments say what the code does and why it must**, in the surrounding
  files' voice. Do not narrate this plan, its tasks, the prototype or the
  user's rounds in them. A comment that a change makes false is corrected,
  or deleted, in the same commit.
- **The names are exact**: the functions, props, classes, test ids, tokens
  and constants below. A test's plumbing (helper names, fixtures, `expect`
  texts) is the implementer's to fit to the code as it is.

## File map

| file | task | what |
|---|---|---|
| `web/src/tiles.ts` | 1, 2 | `RailShare`, `shownShare` (1); `STRIP_DRAG_SLOP` (2) |
| `web/src/Filmstrip.tsx` | 1, 2 | the `onShown` prop and when it is told (1); the drag effect (2) |
| `web/src/Chrome.tsx` | 1 | the mark, its width and place, the mirrored line |
| `web/src/styles/reader.css`, `tokens.css` | 1 | `.reader-progress-shown`, `--reader-mark` |
| `web/src/tiles.test.ts`, `Filmstrip.test.tsx`, `Chrome.test.tsx`, `index.css.test.ts` | 1, 2 | node and jsdom cases |
| `web/src/Filmstrip.browser.test.tsx`, and a browser test of the bottom bar | 1, 2 | real layout, real mouse |

---

### Task 1: The mark is held by tests, and is told when the rail's width changes

**Starts from:** `52a4f9cc`, which has the mark as the user tried it.

**What is on the branch (read it before writing a test):**

- `tiles.ts`: `interface RailShare { start: number; end: number }` and
  `shownShare(rail: { scrollLeft: number; clientWidth: number; scrollWidth: number }): RailShare`,
  the stretch of its whole length the rail shows, as fractions from the
  start of the volume. `Math.abs(scrollLeft)`, clamped to the rail's travel;
  `{ start: 0, end: 1 }` where nothing overflows.
- `Filmstrip.tsx`: the optional prop `onShown?: (shown: RailShare) => void`,
  told from the scroll listener's frame and after every render, and only
  when the share has changed from the one last told.
- `Chrome.tsx`: state `shownOfRail`; inside `.reader-progress` a
  `<div className="reader-progress-shown" data-testid="strip-shown">`, drawn
  only where `end - start < 1`; its width `markWidth(shown)`, which is
  `max(32px, <an eighth of the share>%)` (`SHOWN_MARK_SHARE = 1 / 8`,
  `SHOWN_MARK_FLOOR = 32`); its `left`
  `calc(<alongTheVolume(shown)> * (100% - <that width>))`, where
  `alongTheVolume` is `start / (1 - (end - start))`, 0 at the start of the
  volume and 1 at its end; one pixel proud of the line above and below. The
  line itself takes `transform: scaleX(-1)` where `stepFor(1, reading)` is
  not `1`, so line, fill and mark run the way the strip runs.
- `reader.css`: `.reader-progress-shown { background: var(--reader-mark); border-radius: 2px }`.
  `tokens.css`: `--reader-mark: rgb(236 228 212 / 0.5)`, beside the reader's
  own three.

**The one change to production code this task makes** (spec §2.1, "When it
is told"): the share is also told when the rail's own width or its content's
width changes with no scroll and no render. The prototype missed it: on a
volume whose thumbnails load late the mark was measured at 97.6 px where
91 px was due. Write the test first and watch it fail on `52a4f9cc`. A
`ResizeObserver` on the rail sees its own width; the rail's capture `load`
listener, which already exists for the unit refinement, sees a thumbnail
taking its real width. Use what the tests show is needed and say which in
the report. Nothing here may tell `Chrome` when nothing changed: `Chrome`'s
state must not turn over on a render that moved nothing.

- [ ] **Step 1: `tiles.test.ts`, `shownShare`.** Cases: at the start; in the
  middle; at the end; a reversed row's negative `scrollLeft` giving the same
  answer as its positive twin; a `scrollLeft` past either end, clamped;
  nothing overflowing (`clientWidth >= scrollWidth`, and `scrollWidth` 0).
  Mutations: `Math.abs` removed; the clamp removed; `end` computed without
  `clientWidth`.
- [ ] **Step 2: `index.css.test.ts`, the token.** Hold `--reader-mark` to
  the Ink half of `--text` at its opacity, the way the file holds
  `--reader-hair`. Mutation: the token's colour changed; its opacity changed.
- [ ] **Step 3: `Chrome.test.tsx`, the mark.** Drive `Chrome` so its
  `Filmstrip` tells a share (the simplest honest way the file allows; say
  how). Cases: the mark's `left` and `width` for a share where the eighth
  decides and for one where the floor decides; no mark where the share is
  the whole (`start 0, end 1`); the mark is a child of `.reader-progress`;
  the line carries `scaleX(-1)` for `rtl` and no transform for `ltr`.
  Mutations: the floor dropped; the eighth changed; `left` from `start`
  alone; the mark drawn for a whole share; the mirror taken away; the mirror
  applied in both directions.
- [ ] **Step 4: `Filmstrip.test.tsx`, the telling.** Cases: told after a
  scroll, with the share `shownShare` gives; not told again by a scroll or a
  render that left the share where it was; told after a render that changed
  it; the new case for this task's change. jsdom has no layout, so give the
  rail its three numbers by hand as the file's wheel cases give `scrollLeft`.
  Mutations: the "changed" comparison removed; the call taken out of the
  scroll frame; the after-render call removed; the new telling removed.
- [ ] **Step 5: the browser project.** With a real layout, in both reading
  directions: with the rail at the start the mark is flush with the line's
  start (which is the line's right end for `rtl`), and with the rail
  scrolled to its end, flush with the other end; the mark's centre moves the
  way the strip's content moves; and after a thumbnail loads late the mark's
  width is what the rail's numbers then give. Put these where a bottom bar
  is mounted with a real strip (`App.browser.test.tsx` mounts one; a file of
  the bar's own is also fine) and say which. Mutations: the mirror removed
  (the `rtl` case must fail); `alongTheVolume` returning `start`; the new
  telling removed.
- [ ] **Step 6: whole projects**, per Global Constraints, coverage included.
- [ ] **Step 7: commit**, one commit, a plain sentence, for example "The
  mark on the progress line is held by tests, and follows the rail's width
  (O216)".

**Report** (file `o216-task-1-report.md`, path given at dispatch): status;
commit; each project's summary line; each mutation, caught or NOT caught,
with its first failure line; where the browser cases live and why; what the
new telling uses; anything in the prototype's code a test showed wrong.

---

### Task 2: A mouse drags the strip

**Starts from:** Task 1's head with the controller's commit of the drag as
the user tried it on top (`STRIP_DRAG_SLOP = 5` in `tiles.ts`, and an effect
in `Filmstrip.tsx` with listeners on the rail for `pointerdown`,
`pointermove`, `pointerup`, `pointercancel`, a capturing `click`, and
`dragstart`).

**What the spec asks (§2.2), each a case:**

- A mouse, primary button, only. A touch or pen pointer, and a mouse's other
  buttons, are left alone.
- A press that travels `STRIP_DRAG_SLOP` along the rail becomes a drag: the
  rail's `scrollLeft` is its value at the press less the travel, in either
  reading direction; the pointer is captured; the rail's cursor is
  `grabbing` and is put back at the release.
- A drag never turns the page: the click after the release reaches no tile.
- A press that travels less is a click and turns the page.
- The swallowing does not outlive the release: the next real click works.
- `pointercancel` ends a drag as a release does.
- The browser's own drag of a thumbnail is prevented on the rail.
- The listeners are gone after unmount.

- [ ] **Step 1: `Filmstrip.test.tsx`, jsdom.** One case per bullet above
  that jsdom can ask (it has no layout and no real capture: say which
  bullets it cannot). Dispatch real `PointerEvent`s and `MouseEvent`s at a
  tile inside the rail, since a real press lands on a tile. Mutations: the
  slop comparison removed; the `pointerType` test removed; the `button` test
  removed; `- travelled` as `+ travelled`; the click not stopped; the flag
  never cleared; the flag cleared before the click can arrive;
  `pointercancel` not handled; `dragstart` not prevented; the cleanup
  removed.
- [ ] **Step 2: `Filmstrip.browser.test.tsx`, a real mouse.** In both
  reading directions: press on a tile, move along the rail by a known
  distance in steps, release: the rail's content has followed the pointer by
  that distance, the current page is the one it was, and the cursor was
  `grabbing` on the way. Carry a drag outside the strip's box and back: it
  keeps following (the capture). A press and release with no travel turns
  the page. Mutations: the capture removed; the sign flipped; the click not
  swallowed; the slop removed.
- [ ] **Step 3: whole projects**, per Global Constraints, coverage included.
- [ ] **Step 4: commit**, one commit, for example "A mouse drag of the strip
  is held by tests in both reading directions (O216)".

**Report** (file `o216-task-2-report.md`): as Task 1's, and which of the
spec's bullets no test could be made to hold, with why.

---

### Task 3: A let-go drag carries the strip on

Added on 2026-10-12, after the user tried the drag and asked for momentum
(spec §8).

**Starts from:** the controller's commit of the glide as the user tried it:
an addition to the drag's effect in `Filmstrip.tsx`, with its constants
beside `railStyle`. Its comments are marked `PROTOTYPE` and are to be
rewritten.

**What the spec asks (§8), each a case.** Read §8 whole; in short:

- A release while the pointer is moving starts a glide: speed over the last
  100 ms of the drag, travel of speed × 325 ms in all, an exponential
  slow-down with that time constant.
- No glide: the pointer rested 60 ms before the release; speed under
  0.1 px/ms; a `pointercancel`; the reader has animation off; the system
  asks for reduced motion.
- A glide stops: at an end of the rail; at any press on the rail, whatever
  its pointer type; at a wheel over the rail; at a recentre; when the
  reading direction changes; on unmount.
- A press that stops a glide is not a click: its click is swallowed, the
  focus it took is handed back, and it can still become a drag.
- A mouse only. No render.

**What this task changes in production code, beyond holding the prototype:**

- The arithmetic moves to `tiles.ts`, where a node test can hand it plain
  numbers, beside `STRIP_DRAG_SLOP`: the four constants (`STRIP_GLIDE_MS`,
  `STRIP_GLIDE_SAMPLE_MS`, `STRIP_GLIDE_REST_MS`, `STRIP_GLIDE_MIN_SPEED`),
  `releaseSpeed(trail, releasedAt)`, the pointer's speed in px a millisecond
  at a release or 0 where there is to be no glide, and
  `glideTravel(speed, elapsed)`, how far a glide begun at `speed` has
  carried the rail `elapsed` milliseconds in.
- The reader's animation setting. `Chrome` already has `isAnimating(motion)`
  for its own fade; it tells the strip through a new optional prop of
  `Filmstrip`, `glides?: boolean`, which is true where it is not given. The
  system's setting is still asked through `matchMedia`.
- A change of reading direction stops a glide. The prototype does not.
- The prototype's `PROTOTYPE` comments are rewritten to the Global
  Constraints' rules.

- [ ] **Step 1: `tiles.test.ts`.** `releaseSpeed`: a steady trail; samples
  older than the window not counted; a rest before the release; one sample
  or none; a speed under the floor, either way; a trail whose first and last
  samples share a time. `glideTravel`: nothing at 0 ms; the sign of the
  speed; close to speed × `STRIP_GLIDE_MS` after a long time and never past
  it; never backwards as time goes on. Mutations: the window not applied;
  the rest not applied; the floor not applied; the sign lost; the travel
  growing without bound.
- [ ] **Step 2: `Filmstrip.test.tsx`, jsdom, both reading directions.** With
  a clock and animation frames the case controls: a moving release writes
  `scrollLeft` on in later frames, the other way to the pointer's travel; a
  rested release, a slow one and a `pointercancel` write nothing; frames
  stop being asked for once the glide has run out, and once the rail will
  not go further; a press of each pointer type, a wheel, a recentre, a
  change of direction and an unmount each stop it; a press that stopped a
  glide and is let go where it landed turns no page and leaves focus where
  it was before, and one that goes on past the slop drags; `glides={false}`
  and a system asking for reduced motion each give none; a glide commits no
  render. Mutations: one for each clause of that sentence.
- [ ] **Step 3: the browser project, a real mouse, both directions.** A
  flick carries the rail on after the release and it comes to rest; a
  release after a rest leaves it where it was; a press during a glide stops
  it and turns no page; a glide toward the near end stops at it; and in the
  bar, the mark follows a glide. A real flick's speed depends on the clock
  between its last moves and its release, so say in the report how each case
  is kept from failing by a stall, and confirm none can pass because of one.
  Mutations: no glide at all; the glide's direction turned; a press that
  does not stop it; a press that stops it and still clicks.
- [ ] **Step 4: whole projects**, per Global Constraints, coverage included.
- [ ] **Step 5: commit**, one commit.

**Report** (file `o216-task-3-report.md` in the task's worktree): as Task
2's, and which of §8's clauses no test could be made to hold, with why.

---

### Task D: the documents (the controller's)

- `docs/status.md`: O216 closed, with the user's two readings of 2026-10-12
  in their words, the choice, and what the build measured; a new row for the
  spec's §5 defect; the counts and the closed list; the web test counts.
- `docs/notes.md`: a section with the hand check, the prototype's rounds and
  measurements, and §5's readings.
- Mark in place any prose outside the table that speaks of O216 as open.

## Finishing (the controller's)

1. A whole-branch review of `1eb5723e..HEAD`, with the reviewer's own
   mutations, in both reading directions and in the built app.
2. The user tries the final build in Firefox on the hand-check server.
3. The gate: `env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh`,
   its exit status read from a file and its `all green` line quoted.
4. The PR body, then the claim check with the PR body in its first pass,
   repeated on each correction commit until a pass is clean.
5. Ask before pushing. The PR and the merge are each a further yes.

## Changed while building

(Recorded here as it happens: any departure from a step, and why.)

- **The prototype was the controller's, and four things in it were wrong.**
  The mark went through React state, so a scroll rendered the bars and the
  strip on each frame it moved the mark (Task 1's implementer raised it; fix
  round 1). A
  movement under a two-thousandth of the rail's length was not told, so on
  a short volume the mark stopped short of the line's end (Task 1's review;
  fix round 2). A press let go where the rail could not hear it was left
  standing (Task 2's implementer found it with a real mouse). And a drag
  left focus on the tile it began on, so the next Space or Enter turned to
  that page (the whole-branch review; one fix round after it). The spec's
  §7 has each as a rule.
- **Task 1, Step 5 said "the mark's centre moves the way the strip's content
  moves", which is backwards.** The content moves one way and the stretch
  shown the other. What is held is that the mark moves toward the side the
  later pages lie on.
- **Task 1's fix round 1 asked for a stable function identity for the
  mark's callback, and its review found nothing reads one.** It was taken
  out again in fix round 2, with the mock that held it.
- **Task 2 ran beside Task 1's review and its second fix round**, since
  Task 2 was told to keep to the drag's effect and its own test blocks. The
  whole-branch review was Task 2's first review.
- **The fix round after the whole-branch review first forgot an undragged
  press when the pointer left the rail.** Its own review found that this
  turned a press that slips off the strip's edge and back into a click,
  which turns the page. The release is heard on the window in its place.
- **Task 3 was added after the user tried the built drag** and asked for
  momentum. The spec's §2.2 and §3 had said there would be none.
- **The glide's prototype was the controller's too, and Task 3 found it
  wrong in its turn.** A press that stopped a glide could leave part of the
  strip with no tiles, since focus handed back moved the strip's window to a
  tile the glide had carried the rail on from: focus the rail hands back
  moves no window now, the drag's included. A frame stamped before the
  release moved the rail back. A rest of exactly 60 ms carried. The glide
  also broke the drag's own browser cases, which now mount with `glides`
  off. And a click was swallowed for about two seconds after a flick, which
  the task's implementer raised and one fix round closed (spec §8). The
  glide's review then found no defect in what a glide does, five lines no
  test held, and two small behaviours (a speed read from too little where a
  press had only just become a drag, and `Tab` onto the strip in mid-glide),
  which a second fix round closed.
- **`web/src/testing/mouse.ts` is new**: three browser files needed the same
  real mouse (press, moves and release sent through the browser's own input
  protocol, since `userEvent` has no press held across moves).

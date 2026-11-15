# Compact series tiles: O283, the rest of O274, and O285 opened — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A volume's grid tile on the Series page becomes its cover and its title, with `i` on the cover; the panel `i` opens lies over the rows below, two tiles wide, and holds a Title field that saves as it is left, the alignment detail and ↓ Download. O274 and O283 close, O285 opens.

**Architecture:**
- Task 1: the Title field wraps its whole title and is as tall as it, up to five lines; `layOpen` goes.
- Task 2a: the field saves when left and on Enter, and says "Saved"; Save and `p.current` go.
- Task 2b: a title changed while a save is out is sent after it, and the editor hands `Series` a `flush` so no close loses a title.
- Task 3: `i` moves onto the cover behind a hover rule, ⇄ to the cover's top-right, ↓ into the panel's foot, or onto `i`'s corner where a tile has no `i`.
- Task 4: a new `tilePanel.ts` lays the panel over the rows below, two tiles and the gap wide, inside the page area.
- Task 5: the panel's closes in both modes, every one through `choosePanel`, which flushes the editor first.
- Task 6: the scrollbar gutter.
- Task 7: `docs/status.md` and the two specs' pointers.

Web only: no server, API or route change.

**Tech Stack:** TypeScript/React 19 (`web/`); the vitest projects `node`, `jsdom` and `browser` (Chromium, two launches: `*.scrollbars.browser.test.tsx` runs only in the one that shows scrollbars).

**Spec:** `docs/superpowers/specs/2026-11-14-compact-series-tiles-design.md`, approved by the owner on 2026-11-14 and committed at 60f25185 on main 64ffd2b7 (#247's merge, which changed none of the web files below). Every (owner) and (controller) decision in it is settled, its "Settled after review" list included. The prototype's diff, `.superpowers/sdd/compact-tiles-evidence/prototype-round5.diff`, is the reference for fades, offsets and ordering the spec leaves unstated: read it, never copy it (its comments say "O283 prototype"). Line numbers below are hints at 60f25185; locate by symbol.

**Spec gaps found against the code** (each settled in the task named):
1. The hiding rule's condition written as `(hover: hover) and not (any-pointer: coarse)` is not valid Media Queries 4 (`and` takes a parenthesised condition, and `not …` is not one), so Chromium would read it as `not all` and never hide `i`. Written `(hover: hover) and (not (any-pointer: coarse))`. Task 3.
2. `tests/support/pointer.ts`'s `coarseRulesOn()` (:18-36) throws on any `@media` asking about pointer or hover other than `(pointer: coarse)`. The new rule would turn every coarse-pointer browser case red (About, RuleEditor, Config and others), so the helper learns it in the same commit. Task 3.
3. "Escape puts back the stored title, the one held when the field took focus or at its last successful save": the two readings part after a failed save, where the field's text was never stored, and whenever a save is still out as the field takes focus again. The plan takes the stored title, the spec's noun (`:124-125`); the spec is newer than the phone fixes' "whichever came later" and owner-approved. Task 2a.
4. Settled item 4 is silent on a save that fails: nothing queued is sent after a failure, which stays shown; the next leave or Enter sends the newest text. Task 2b.
5. On-select's Escape sends focus to the tile's link, and focus on the link fires `onFocusCapture`'s `reveal` (`Series.tsx` ~609-614), which would open the panel again. And a `tile-unreadable` tile (~636-648) has no link. Task 5 adds a `refocusing` guard and a fallback target.
6. The spec gives on-select two of `button`'s closes, Escape and an outside press. Focus arriving outside closes only in `button` mode; in on-select, focus on another tile moves the panel, as now. Task 5.
7. Biome's `useComponentExportOnlyModules` (`web/biome.jsonc`) forbids exporting the placement function from `Series.tsx`. DOM-reading code there that jsdom cannot reach would also pull `Series.tsx` under its per-file coverage floors (`scripts/coverage-floor.mjs`). So the placement lives in a new `web/src/tilePanel.ts`. Task 4.
8. A child's layout effects run before its parent's, so `fitTitle` fits the field before `Series` sets the panel's width. The panel's CSS therefore carries the two-tile width before the measure (the prototype's `calc(200% + 1rem)`, and `+ 0.6rem` under `.library.compact`), and the field's width observer catches the rest. Task 4.
9. `.cover-box .aligned` (`library.css` ~229) also draws the library grid's ⇄ (`SeriesGrid.tsx` ~44). Its move to the top-right is scoped to `.volumes`. Task 3.
10. O283 has a row and no bullet (`docs/status.md` ~1908), while the spec puts the reopened settlement "in O283's bullet". Task 7 writes the bullet. The spec gives dated pointers to "both specs" (style-pass and phone-fixes). Download §8, also reversed, gets no pointer, and its reversal is recorded in O283's bullet.
11. The spec is silent on an Escape while the field's own save is out. It puts back the title being sent, which becomes the stored one when that save lands; if the save fails, the failure shows and a later Escape puts back the stored one. Putting back the stored title instead would let the next leave send the older title over the newer. Task 2b.
12. The outside press is a `pointerdown` (spec, "a press", as `usePopover`), which also starts a scroll, and a press on `main`'s classic scrollbar can arrive as one whose target is `main`. A press whose target is the scroller itself (`.library > main` or the root element) is therefore ignored, so reaching ↓ Download by the scrollbar (Review Focus 4) keeps the panel. A press on the page area's bare background then closes nothing (Escape or another press does), and a touch drag that starts outside still closes it, as View's does. And a close waits on `flush` while focus stays in the field, so text typed meanwhile could be unmounted unsaved: after `flush` answers `true`, the close asks it again and goes ahead only on `null`. Task 5; Task 6 (e) presses the scrollbar.
13. While a panel is open, its capture listener takes every Escape on the page wherever focus is (an on-select panel opened by hover sends focus to its tile's link), and with View open over it one Escape closes both, since `stopPropagation` does not stop another listener on `document`. The spec's "caught on the way down at the document … as `popover.ts`'s `usePopover` does" sanctions it, and View's own listener takes every Escape too: recorded, not narrowed. Task 5.

## Global Constraints

- **Comments.** A comment says why: the reason, the invariant, what breaks if it changes. Never history or measurements; it may point at an O-number. No ISO date unless it follows `ruling of`. A new comment block holds at most 8 lines of text. `node scripts/check-comments.mjs`, run from the repo root, checks this. If it fails on an area's comment share, follow `docs/ci.md`, "The comment guard", and say so in the report. A comment that names the 2026-09-18 settlement, O274's open half, `layOpen`, Save, or an open box is rewritten by the task that changes its code.
- **Attribution.** None anywhere: code, comments, docs, commits.
- **Copy, exact:** `Saved`; `↓ Download` (panel); `↓` (corner); `i`'s `aria-label` `details for ${w.title}`; both downloads keep `aria-label` `download ${w.title} — ${shown.label}`. Tests write each string out, never compute it through the code under test.
- **Pinned values** (spec, "The design as decided"):
  - `i`: on the cover's bottom-right, 0.25rem in from both edges at any column width. A sibling of the tile's link, never inside it. At least 24×24 px, 44×44 under `(pointer: coarse)`. It keeps `aria-expanded`.
  - Under `(hover: hover) and (not (any-pointer: coarse))`, `i` is hidden by `opacity: 0` and `pointer-events: none`, never by `display` or `visibility`, until its `li` is `:hover` or `:focus-within`, or it is `[aria-expanded="true"]`. It fades with `transition: opacity 120ms`, and with none under `(prefers-reduced-motion: reduce)`, a rule with no hover condition in it.
  - ⇄ in the Series grid: the cover's top-right, 0.25rem in. The table's inline ⇄ and the library grid's are unchanged.
  - ↓ in the panel: last child, text `↓ Download`, `font-size: var(--text-ui)` (14 px), at least 24 px tall (44 coarse). Its left padding is 2 px less than its right (the prototype's `calc(0.6rem - 2px)` and `calc(0.6rem + 2px)`). It keeps today's `href={editionDownload(w.slug, shown.key)}`, bare `download`, `aria-label` and guards (`mayDownload && shown && shown.missing === 0`).
  - ↓ on the corner: where a tile has no `i` (`panel` is `'off'` or `'on-select'`). It takes `i`'s place, size, inset and hover rule, with the same `href`, `download`, label and guards. An on-select panel draws no ↓.
  - Panel: 4 px (`--space-1`) under its tile, over the rows below. Its width is two tiles and the grid's column gap, or the page area's content width where that is less. It runs from the tile's left edge, or back from the tile's right edge where rightward would pass the area's end, and never starts left of the area.
  - Panel stacking and look: `z-index: 5`, above the covers and their marks, under `.phone-bar` (10) and `.view-panel` (15). Drawn as `.view-panel` is: `padding: var(--space-3)`, `1px solid var(--line-strong)`, `var(--surface)`, `box-shadow: 0 4px 16px var(--scrim)`.
  - Panel contents: children 12 px apart (`--space-3`), in this order: the Title form, the stamp section, the alignment detail, ↓ Download. The alignment detail drops its own border, margin and padding.
  - Title field: the label, then 4 px (`--space-1`), then the field. The field is as wide as the panel, with 8 px (`--space-2`) side padding, and wraps its whole title at rest. Its height is its title's lines, up to five, and past that it scrolls. `Saved` is muted (`--muted`, `--text-meta`) at the end of the label's line and takes no row. It clears after 3 s and at the next edit. A failure and the unwritten reasons sit under the field, 8 px (`--space-2`) apart.
  - Gutter: `scrollbar-gutter: stable` on `.library > main:has(.volumes)`, and under `(width < 900px)` on `:root:has(.volumes)`.
- **Web checks per task.** Run from `web/` by absolute path, never a bare `cd` (it is zoxide here), never `pnpm exec`. `E=/home/davey/projects/taiyaku/.superpowers/sdd/compact-tiles-evidence`, set in every Bash call that uses it.
  - `env -C /home/davey/projects/taiyaku/web ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`. Read the summary lines from the log, never a pipe through `tail`. Use `env -C`, never `cd`/`builtin cd`, for every command that must run in `web/` or the repo root: a `cd` has moved a shell's directory for later calls here.
  - `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`, each under `env -C /home/davey/projects/taiyaku/web`.
  - Coverage: `./node_modules/.bin/vitest run --project node --project jsdom --coverage --reporter=default`, then from the repo root `node scripts/coverage-floor.mjs`. A file under its floor gets a test of the kept code; a floor is never lowered.
  - Whole projects only as a task's final check. `-t` is for watching one mutation fail, and for running a real-key case alone.
  - A task that touches CSS or a `*.browser.test.tsx` runs the whole browser project, which CI never runs.
- **Gate** (Finish only, and after any late code fix): `env -C /home/davey/projects/taiyaku -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/gate.log 2>&1; echo "exit $?" >| $E/gate.exit`. Run the repo's own script, never a copy, at load < 24 (`cat /proc/loadavg`). Quote the `all green` line.
- **Browser cases.**
  - Geometry is asserted only in the browser project; jsdom lays nothing out.
  - "Nothing moves" is rect equality (`toJSON()`); edges are otherwise within 0.5 px.
  - Each case first asserts what makes right and wrong render differently: a tile in the row's last column, a row below the open tile, a title wider than the field on one line. The O274 cases in `MetadataEditor.browser.test.tsx` show the shape.
  - Real keys: focus an element in the page before the first real key; a key pressed before anything in the test frame has focus goes to the frame. Each real-key case also passes alone with `-t` (a regex: `.` for punctuation).
  - Pointer and hover: from Task 3 on, a case that depends on them calls `onTestFinished(pointerRulesAs(…))` first. Never Chromium's touch emulation: switched off again, it leaves later files with no pointer.
  - On a mouse device, hover a tile before a real click on its `i`.
  - Read `opacity` only after finishing the element's transitions (`for (const a of el.getAnimations()) a.finish()`).
  - Click by role or by element, never by text that CSS draws (`@vitest/browser` reads `content:` into names).
  - Load the UI face before the editor first mounts (`document.fonts.load` on the form's font): `fitTitle` measures on mount, and a face that lands later changes no width.
  - Chromium crashes: a red browser run with import or connection errors and no failed assertion is headless Chromium crashing. Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Watched mutations.** Every new or tightened assertion is watched failing. Put the named defect in by hand, watch the test go red on its assertion (not on a throw from setup), restore it, and check the restore with `git diff`. Record each in the task report: the defect, the test, the message.
- **Pre-existing failures** count only if the same command fails at main 64ffd2b7, checked in a detached scratch worktree under the session scratchpad. Copy `web/node_modules`, never symlink it, and copy `crates/taiyaku-wasm/pkg`. Report them; do not fix them.
- **Classes.** `classNames.test.ts` fails on a class no selector names, and on a `HOOKS` exemption whose class gains a rule or leaves its file. Move or delete the entry with the test that selects by it.
- **Commits.** One per task, its subject naming the O-number(s). Never `--amend`. Before committing, check the previous task's hash is an ancestor of `HEAD`. Branch `compact-tiles`, in the main checkout, no worktree. Each task's report goes to `$E/T<n>-report.md`, written by Bash (`cat >| … <<'EOF'`); the Write tool is refused under `.superpowers/`.
- **No Rust.** If a task finds it needs a server change, it stops and reports.
- **Not in this plan** (spec, Non-goals): trimming `.cbz` from titles (the prototype's `shownTitle` and `saveShown`; that is O284's), the cover-size slider (O285 is only opened), and the table's route to alignment detail (O63).

## Review Focus

1. **A title with no space that runs past five lines** (a file name): the panel stays two tiles and the gap wide, nothing in it runs past its edges or scrolls the page sideways, and the field is five lines tall and scrolls inside. Task 4, case (k).
2. **A save racing a close**: an outside press while the field's own save is out sends one PATCH. The panel stays until the save lands, then closes; on failure it stays, with the failure under the field. Task 5, case (m).
3. **A touch laptop** (a primary pointer that hovers, a coarse `any-pointer`): `i` is always shown at 24 px, and a tap on it lands on `i`, not the cover's link. Task 3, case (f).
4. **A panel opened in the last row of a page that did not scroll, a one-volume series included**: the page gains a scrollbar, no tile moves or narrows, and scrolling brings ↓ Download into view, a press on the scrollbar included. Task 6, cases (a)–(c) and (e).
5. **An unreadable tile** (`tile-unreadable`, no link): `i`, ⇄ and the corner ↓ sit on its cover as on a readable tile. In on-select mode, Escape from its panel leaves focus on an element in the page and does not reopen the panel. Task 3, case (a); Task 5, case (h).

---

### Task 1: The Title field fits its title (O283)

Spec, "The Title field": wraps the whole title at rest as while edited; as tall as the title up to five lines; re-fitted as typed and as the width changes; never enlarges, opens or moves when clicked.

**Files:**
- `web/src/MetadataEditor.tsx`:
  - Delete `contentSpan` (~227-236), `layOpen` (~238-281) and the effect under "The Title box opens while it is edited" (~554-579).
  - Rename the `open` state to `editing`, and `data-open` to `data-editing`. Its life is unchanged: set on focus and by `edit`, cleared on Escape and blur.
  - Add the module-private `fitTitle(box: HTMLTextAreaElement): void`. It applies `layOpen`'s height arithmetic (~265-276) at the box's own width: one line read at `rows={1}` with the inline height cleared, the height read with `overflow-y: hidden`, capped at five lines, and `scrollTop` kept.
  - A `useLayoutEffect` on `[value]` fits the box and watches it with a `ResizeObserver` (only where `typeof ResizeObserver !== 'undefined'`). It re-fits only when `clientWidth` changed, since setting the height fires the observer again. Its body does not read `value`, so it carries a `biome-ignore lint/correctness/useExhaustiveDependencies` naming `value` as a trigger, as the deleted effect's (~555) does.
  - The comments this makes false are rewritten: the textarea's (~876-877, "at rest it is drawn as one line") and `edit`'s doc (~598, "opens the box again").
- `web/src/styles/library.css` (~451-479): the two Title-box rules become one `.metadata-editor textarea` rule: `box-sizing: border-box; width: 100%; padding: 1px var(--space-2); vertical-align: top; resize: none; overflow-y: auto; white-space: pre-wrap`. Its comment gives the why (O283). The `[data-open]` rule goes.
- New `web/tests/support/lines.ts`:
  - `oneLineIn` (module-level, ~204), `lineOf` and `pastTheEdgeOf` (both inside "a stamp plan drawn in a volume's tile (O274)") move here from `MetadataEditor.browser.test.tsx`, which imports them.
  - `lineCount(field: HTMLTextAreaElement, text: string, width?: number): number`, the "past five lines" case's probe (~812-830) made shared. A hidden `div` appended to the field's `label`, never inside the field: a textarea's children are not laid out, so a probe there reads 0. It is `width` px wide (by default the field's content width), `white-space: pre-wrap; overflow-wrap: break-word`, in the field's computed font, and answers its height over the field's line-height.
- `web/tests/MetadataEditor.browser.test.tsx`, `web/tests/MetadataEditor.test.tsx`.
- `web/tests/MetadataEditor.scrollbars.browser.test.tsx`: its `linesAt` probe becomes `lineCount`.
- `web/tests/Series.onSelect.browser.test.tsx`, its `LONG_TITLE` doc (~20, "open, the Title box hangs over the next tile") included.

**Interfaces:**
- Produces: the field's height follows its own width. Task 4 sets the panel's width after the first fit and relies on the re-fit.
- Produces: `data-editing` on `textarea[name='title']`. Task 2a keeps it; Task 5 reads it.
- Produces: `tests/support/lines.ts` as above. Tasks 2a and 4 import from it.

- [ ] **Step 1: Failing tests.**
  - New describe "the Title field fits its title (O283)" in `MetadataEditor.browser.test.tsx`. Its helper mounts the editor in a `div` of a given width inside `.library`. Line counts come from `lineCount`.
  - (a) `it.each` over titles of 1, 3 and 8 lines, "is as tall as a title of %i lines, up to five, at its width": `lineCount(field, title)` gives that count first. Then `clientHeight` less padding equals `min(n, 5) ×` line-height. For 8: `scrollHeight > clientHeight`, and `overflowY` is `'auto'`.
  - (b) "wraps the whole title at rest, 8 px in from each side": never focused. `whiteSpace` is `'pre-wrap'`, `paddingLeft` and `paddingRight` are `'8px'`, and `scrollWidth ≤ clientWidth + 1`. The fixture tells: `oneLineIn(label, title) > lineOf(field)`, probing in the field's `label`, never in the field.
  - (c) "neither moves nor grows when clicked, or typed into without taking another line": the field's and the label's `toJSON()` rects are equal before a real click, after it, and after typing one character at the end that `lineCount` says adds no line.
  - (d) "grows one line as a typed title takes another": type a suffix `lineCount` says adds exactly one line; the height grows by one line-height.
  - (e) "is fitted again when its width changes": the container goes from 18rem to 30rem. `lineCount` at the two content widths differs, asserted first. The height then equals the count at 30rem.
  - (f) Move the paste case ("a pasted line break is a space…") into this describe, unchanged but for its mount.
  - Scrollbars file: its case becomes "is as tall as its title's lines at its width, beside a scrollbar's width". It mounts in a fixed-width container, with no focus or click, and keeps today's assertions.
  - jsdom: the describe "the open Title box's watch on its page area (O274)" and its case become "watches its own field for a width change, and lets the watch go when the editor goes". With the `Watch` stub: after mount, `[{ observed: [field], disconnected: false }]`; after unmount, `disconnected: true`.
  - Run node, jsdom and browser: (a)'s 3- and 8-line rows, (b), (d), (e), the scrollbars case and the jsdom case fail. (a)'s 1-line row and (c) pass first, since the helper mounts no `.volumes` and `layOpen` never opens the box; Step 3's mutations show them.
- [ ] **Step 2: Implement.**
  - Delete the describe "the Title box opens downward while it is edited (O274)"; only the paste case lives on, in (f).
  - In `Series.onSelect.browser.test.tsx`, delete `typedWithPointerOnOverhang` and its two cases, "…on Escape with the pointer on the box's overhang" and "…on Tab to Save…". Their premise, an open box over the next tile, is gone; Task 5 writes their successors.
  - Run all three projects to green.
- [ ] **Step 3: Watched mutations.**
  - Cap at `Infinity` lines: (a)'s 8 fails.
  - `white-space: pre` at rest: (b) fails.
  - Add `.metadata-editor textarea:focus { width: 200% }`: (c) fails.
  - Fit on mount only (drop the `value` trigger): (d) fails.
  - Drop the observer: (e) and the jsdom case fail.
  - Measure without `overflow-y: hidden`: the scrollbars case fails (its fixture asserts the extra line at a scrollbar's width).
- [ ] **Step 4:** `tsc -b`, `biome check`, coverage and floor, the comment check. Commit `… (O283)`. Write `$E/T1-report.md`.

---

### Task 2a: The Title field saves as it is left (O283)

Spec, "The Title field" and settled items 5 and 6; gap 3 above. Save and `p.current` go. The field saves when left (blur) or on Enter. An unchanged title is not sent. A blank one puts back the stored title and sends nothing. One save is out at a time: in this task a leave or an Enter while one is out sends nothing, as an Enter does today (`titleKeys`' `if (!saving)`); Task 2b sends the newer text once the save returns.

**Files:**
- `web/src/MetadataEditor.tsx`:
  - Delete the `current` state, the Save button and `p.current` (~874-877 at Task 1's head).
  - `saving` becomes `out: useRef<Promise<boolean> | null>`.
  - `held` becomes `stored`: the mounted title, then the text of each save that succeeded. It is never the server's re-projected title (item 6), and never set at focus: the `onFocus` write goes, and its doc ("whichever came later") is rewritten. Escape restores it.
  - One `saveTitle(): Promise<boolean> | null` serves Enter, blur and the form's `onSubmit` (the `<form>` stays, without a submit button, so `submit(form)` in tests still saves).
  - `saved` state and its 3 s timer, as in the prototype. It is cleared by `edit` and when a save starts.
  - `<p className="saved-note" role="status">{saved ? 'Saved' : ''}</p>`, inside the form, right after the label, rendered from the first render.
  - The comments about the stamp section's place outside the form (~928-933) say "a title save" instead of "pressing Save".
- `web/src/Series.tsx`: the comment at the `MetadataEditor` mount (~731-733, "the editor shows the server's re-projected title itself meanwhile") says the field keeps the typed title (item 6).
- `web/src/styles/library.css`:
  - `.metadata-editor`: `position: relative; display: flex; flex-direction: column; gap: var(--space-2)`, its `p` margins 0.
  - `.metadata-editor .field.title`: a flex column, `gap: var(--space-1)`.
  - `.metadata-editor .saved-note`: `position: absolute; top: 0; right: 0; color: var(--muted); font-size: var(--text-meta)`, with one line-height shared with the label's span.
- Tests: `web/tests/MetadataEditor.test.tsx`, `web/tests/MetadataEditor.browser.test.tsx`, `web/tests/Series.test.tsx`.

**Interfaces:**
- Consumes: `data-editing` and `tests/support/lines.ts` (Task 1).
- Produces, inside `MetadataEditor`, for Task 2b: `saveTitle(): Promise<boolean> | null`. A blank title is put back first. It answers `null` and sends nothing when the field's text equals `stored`, is blank, or a save is out; otherwise it sends the text and answers that save: `true` once stored (`stored` and `Saved` set), `false` once it failed (the failure shown under the field). With it, `out` (the save out, `null` when none) and `stored`.
- Keeps `data-editing` as Task 1 left it; Task 5 reads it.

- [ ] **Step 1: Convert the cases that press Save.** In `MetadataEditor.test.tsx`, replace `submitButton` and its clicks with two helpers: `enterWith(host, text)` (focus, `typeInto`, Enter) and `leaveWith(host, text)` (focus, `typeInto`, blur). Type a title that differs from the mounted one, because an unchanged title is not sent.
  - "saves a corrected title as the field is left, and says Saved" (was "…and shows the new one"): the patch is called with `['w', { title: 'Volume 1' }]`, and `[role="status"]` reads `Saved`.
  - "keeps the typed title in the field, not the server's re-projected one" (was "shows the server-reprojected title…"): `field.value` is `'Volume 1'`, and there is no `p.current`.
  - "says so when…", "says nothing about being unwritten…", "names every reason…", "surfaces a hard failure…" and "stringifies a rejection…": use `enterWith`.
  - "renders exactly what an ordinary successful save renders…": `enterWith` on each of its two editors, so Enter stays the path under test. Blur the second editor's field before comparing `innerHTML` (focusing the second blurred the first, and the two would differ by `data-editing`), and each `onPatch` is called exactly once.
  - "saves on Enter what the box holds, as Save does, and puts no line break in it" becomes "saves on Enter what the field holds, and puts no line break in it".
  - "clears the last save failure while the next save is out": the second Enter resends, since the text still differs from the stored title. No alert while that save is out; after it, no alert and the status reads `Saved`.
  - "sends no second save for an Enter while one is out, and none after it for an unchanged title": one call before the out save settles, and one after.
  - "puts a pasted title in at the caret…" and "turns each line break…": Enter replaces Save.
  - "saves the typed title when focus leaves the field, and keeps it there" (was "keeps the typed title when focus leaves the box").
  - "puts back on Escape the title the box held when it took focus, and keeps the focus" becomes "puts back on Escape the title saved as the field was last left, and keeps the focus": its blur now saves, so its patch resolves.
  - `Series.test.tsx`, "keeps the series it drew, and the editor open, beside the alert…": the `.current` assertion becomes the field's value, `renamed`.
- [ ] **Step 2: New failing tests, jsdom.**
  - (a) "sends nothing when the field is left, or Enter pressed, with its title unchanged": focus then blur, and focus then Enter; zero calls.
  - (b) `it.each(['leave', 'Enter'])`, "puts the stored title back for a blank title on %s, and sends nothing": `typeInto(field, '   ')` from `'old'`, then the trigger. The value is `'old'`, with zero calls.
  - (c) "says Saved in a status region there from the first render, outside the label": at mount there is one `[role="status"]` with text `''`, `label.contains(status)` is false, and the label's `span` reads `Title`. After a save it reads `Saved`, still outside the label. Never `label.textContent`: React writes the field's value into the textarea's text, so the label's text holds the title too.
  - (d) "clears Saved after 3 s, and at the next edit", with `vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })`: at 2999 ms it reads `Saved`, at 3000 `''`. In a second case, `typeInto` after `Saved` clears it at once.
  - (g) "puts back the stored title on Escape after a failed save": type `X`, blur (the save rejects), focus again, Escape. The value is `'old'`, not `'X'`.
  - (h) "marks the field data-editing from focus until Escape, again at the next edit, and not after blur".
  - (i) "saves a changed title when focus leaves the field for the stamp controls" (item 6): with `canStamp`, focus the plan button after typing. One call, and the stamp plan is asked as before.
  - Run jsdom: (a)–(d), (g) and (i) fail. (h) passes first, since Task 1 sets and clears `data-editing`; its Step 5 mutation shows it.
- [ ] **Step 3: New failing tests, browser.** In `MetadataEditor.browser.test.tsx`, describe "the Title field's label, Saved and what follows it (O283)":
  - (o) "sets the label 4 px above the field": the label span's bottom plus 4 is the field's top.
  - (p) "draws Saved at the end of the label's line, so nothing moves as it comes and goes". Saved's box lies between the label span's top and the field's top, and its right edge is the form's. Its colour is a probe's `color: var(--muted)`. The field's, the label's and the form's rects are equal with `Saved` shown and after it clears.
  - (q) "puts a failure 8 px under the field, and the unwritten reasons too": one save rejects; another answers `persisted: false` with a `why`.
  - Rewrite "draws its label and box in the editor's face, not the tile title's". The label's and the box's faces equal the form's own computed face, and the fixture still shows the tile title differing from the form in all four.
  - In "a stamp plan drawn in a volume's tile (O274)": drop `.current` from "the editor stays inside a right-hand tile…", and use `oneLineIn(label, TITLE) > lineOf(field)` as that part's fixture check, probing in the field's `label` (Task 1's `lines.ts`). In "…a save's unwritten reason…", the Save press becomes typing ` (2)` into the field and a real Enter.
  - Run browser: (o)–(q) fail.
- [ ] **Step 4: Implement**; run node, jsdom and browser to green.
- [ ] **Step 5: Watched mutations.**
  - Drop the unchanged check: (a) fails.
  - Send a blank title: (b) fails.
  - Render the region only once saved: (c) fails. Render it inside the label: (c) fails.
  - Drop the timer: (d) fails. Keep `Saved` on edit: (d) fails.
  - Set `stored` at focus: (g) fails.
  - Keep `data-editing` after Escape: (h) fails.
  - `.saved-note` static: (p) fails. Label gap `--space-2`: (o) fails.
  - The form's gap `--space-3`: (q) fails.
- [ ] **Step 6:** `tsc -b`, `biome check`, coverage and floor, the comment check. Commit `… (O283)`. Write `$E/T2a-report.md`.

---

### Task 2b: A title changed while a save is out is sent after it, and `flush` (O283)

Spec, "Closing" ("no close loses an edit") and settled items 3 and 4; gaps 4 and 11 above. A title changed while a save is out is sent once it returns, if it still differs from the title that save stored: one save at a time, the newest text last. Nothing queued is sent after a failure. An Escape while the field's own save is out puts back the title being sent. The editor hands `Series` a `flush`, which Task 5's closes wait on.

**Files:**
- `web/src/MetadataEditor.tsx`:
  - `saveTitle` (Task 2a): while a save is out, a leave or an Enter no longer sends nothing; the newest text is sent once that save returns, as above, and the call answers that later save. After a failure nothing queued is sent, and the failure stays shown (gap 4).
  - Escape restores the text being sent while the field's own save is out, else `stored` (gap 11).
  - A `latest` ref mirrors `value`, because `flush` is called from document listeners.
  - Export `TitleFlush` (below) and take `ref?: Ref<TitleFlush>`, wired with `useImperativeHandle`. `flush` answers as its doc says, through `saveTitle` and `out`.
- Tests: `web/tests/MetadataEditor.test.tsx`.

**Interfaces:**
- Consumes: `saveTitle`, `out` and `stored` (Task 2a).
- Produces, in `MetadataEditor.tsx`:

```ts
/** What `Series.tsx` holds of the open editor, so that no close loses a title. */
export interface TitleFlush {
  /**
   * A blank title is put back first. Then `null` when nothing typed is
   * unsaved and no save is out; otherwise the save of the newest text, sent
   * now or after the one out: `true` once it is stored, `false` once a save
   * failed, its failure then shown under the field.
   */
  flush(): Promise<boolean> | null
}
```

- Produces: the `MetadataEditor` prop `ref?: Ref<TitleFlush>`. Task 5 passes it.

- [ ] **Step 1: New failing tests, jsdom** (`MetadataEditor.test.tsx`).
  - (e) "sends a title changed while a save is out once it returns, the newest text last, one at a time": a deferred first save. Enter on `'A'`, Enter on `'AB'`, blur on `'ABC'`; the calls are `[['w', {title: 'A'}]]` while it is out. Settle `true`: `[[…'A'], [… 'ABC']]`, and no third.
  - (f) "sends nothing queued after a save that failed, and shows the failure": the first rejects. `'AB'` is not sent and the alert shows; a later Enter sends `'AB'`.
  - (g2) "puts back on Escape the title being sent while the field's own save is out" (gap 11): from `'old'`, type `A`, Enter, the save deferred; type `AB`, Escape. The value is `'A'`. Settle `true`: no second call. A second case, "…and the stored one once that save has failed": asserted first, the first Escape put back `'A'`; the deferred save then rejects, the failure shows, and the next Escape puts back `'old'`.
  - `flush`, through `const handle = createRef<TitleFlush>()`:
    - (j) "flush answers null with nothing unsaved and no save out".
    - (k) "flush sends a changed title and resolves true once it is stored": the save is deferred; it is called once and the promise has not settled until the save does.
    - (l) "flush resolves false when the save fails, and the failure is under the field".
    - (m) "flush waits for the save out and the newest text after it, sending each once".
    - (n) "flush puts a blank title back and answers null".
  - Run jsdom: all fail but (f), which passes first under Task 2a's rule (a save out sends nothing); its Step 3 mutation shows it once the queue is in.
- [ ] **Step 2: Implement**; run node, jsdom and browser to green (the browser project draws the editor).
- [ ] **Step 3: Watched mutations.**
  - Drop the queue (the prototype's behaviour): (e) fails. Send while a save is out: (e) fails.
  - Send the queued text after a failure: (f) fails.
  - Escape restores `stored` while a save is out: (g2)'s first case fails. Keep restoring the sent text after that save failed: its second fails.
  - `flush` always `null`: (k) fails. Resolving `true` on failure: (l) fails.
  - `flush` sends whenever asked, unchanged text included: (j) fails. `flush` answers the save out alone, never sending the newer text after it: (m) fails. `flush` sends a blank title: (n) fails.
- [ ] **Step 4:** `tsc -b`, `biome check`, coverage and floor, the comment check. Commit `… (O283)`. Write `$E/T2b-report.md`.

---

### Task 3: The tile: `i` on the cover, ⇄ at its top-right, ↓ in the panel or on `i`'s corner (O283)

Spec, "The tile", "↓ Download", "↓ where a tile has no `i`", settled items 1 and 7; gaps 1, 2 and 9 above.

**Files:**
- `web/src/Series.tsx`:
  - The ↓ block (~659-686): in `button` mode it leaves the tile and becomes the last child of the open panel, `↓ Download`. Otherwise it stays a direct child of the `li`, after the link or the `tile-unreadable` box, as `↓`. Its why-comments go with it. The anchor is built once (one element, or a helper taking its text) and placed in one of the two spots; two copies of its guard, `href` and `aria-label` would be duplication.
  - `i` (~687-713): `aria-label={`details for ${w.title}`}`.
  - The open panel's `div` (~719) gains `className="tile-panel"`, and `openPanel`'s doc (~494, "its alignment detail and its editor") names ↓ Download too.
- `web/src/styles/library.css`:
  - `.volumes > li { position: relative }`.
  - `.volumes .panel-open` and `.volumes > li > a.get`: positioned with the prototype's round-5 offsets (`top: 0; right: 0.25rem; margin-top: calc(150% - 0.25rem); translate: 0 -100%`). The comment says why 150%: a percentage margin is taken of the width, and the cover is the tile's width at 2:3. `.volumes .panel-open` keeps its `color` and `background`, which `index.css.test.ts`'s D3 contrast cases read; its offsets join that rule or one spelled otherwise, never a second `.volumes .panel-open {` above it, since `ruleBody` reads the first.
  - The corner `a.get` takes `box-sizing: border-box`, as the button `i` has, so that at `i`'s size its padding and border fall inside it.
  - The hiding rule, `@media (hover: hover) and (not (any-pointer: coarse))`, over `.volumes > li > :is(.panel-open, a.get)`, with the three showing arms.
  - The reduced-motion rule.
  - `.volumes .cover-box .aligned { top: 0.25rem; bottom: auto }`.
  - `.tile-panel a.get`: the word button, with `margin-top: 0`; `.volumes a.get`'s `0.15rem` would break Task 4 (p)'s 12 px gap.
  - The `.volumes a.get` comment (~361-370) no longer says the control sits under the tile.
  - Biome's `noDescendingSpecificity` governs the order of these rules (the comment at ~208-211 shows the pattern).
  - If Biome cannot parse the condition, nest `@media not (any-pointer: coarse)` inside `@media (hover: hover)`, key the helper on both texts, and say so in the report.
- `web/tests/support/pointer.ts`: `pointerRulesAs` (below), whose doc names the table. `coarseRulesOn()` stays, as a one-line call of `pointerRulesAs('phone')`, so its callers in the About, App, Config, Licence, RuleEditor and Shell browser tests are untouched. An unknown condition still throws.
- New `web/tests/support/seriesPage.tsx`: `seriesPage` (below), the one Series mount Tasks 3, 4 and 6 share. It follows `Series.onSelect.browser.test.tsx`'s `firstPanelOpen`.
- `web/tests/Series.test.tsx`:
  - "Series, grid layout" mocks `library.work`, as "Series, the work-level editor" does: opening a panel fetches the work, and a bare `vi.fn()` makes `reveal`'s `.then` throw.
  - "Series, the edition selector › in the address": its `page()` sets `panel: 'off'`, so `downloads(host)` reads each tile's corner ↓; its doc says so. Under the defaults (`button`), no tile would carry an `a.get`.
- New `web/tests/Series.tile.browser.test.tsx`.
- `web/tests/index.css.test.ts`: one node case.

**Interfaces:**
- Produces, in `tests/support/pointer.ts`: `export type Device = 'phone' | 'mouse' | 'touch laptop'` and `export function pointerRulesAs(device: Device): () => void`. It switches each pointer or hover `@media` block in place to `all` or `not all`, by a table keyed on the condition as Chromium serialises it, with whitespace removed. For `phone`, `(pointer: coarse)` is true and the hiding rule false. For `mouse`, the hiding rule is true and `(pointer: coarse)` false. For `touch laptop`, both are false. It returns the restore.
- Produces, in `tests/support/seriesPage.tsx`: `seriesPage({ width, height, panel, compact, who }): Promise<HTMLElement>`. It sets the viewport (`height` 800 unless given), restored in `onTestFinished`. It mounts `BrowsePrefsContext` (the defaults with `panel`), `BrowseControlsContext` with `{ change: () => {}, reset: () => {} }` (without it `ViewMenu` draws nothing, `ViewMenu.tsx` ~35) and `GateContext` (`who`) around `.library` (`library compact` when `compact`) with `.sidebar-column` and `main` holding a router at `/series/7`. It returns the host. Each file keeps its own `vi.mock`s and waits for its own tiles.
- Produces: `.tile-panel` on the open panel's wrapper, and `a.get` as its last child in `button` mode. Tasks 4 and 5 use both.

- [ ] **Step 1: Failing tests, jsdom** (`Series.test.tsx`). Each case is per element, and the panel case asserts first that the panel is drawn (`.alignment-panel` present), so it cannot pass vacuously.
  - "offers a download per tile…" becomes:
    - "draws no download on a tile in button mode, and one at the foot of its open panel": no tile has a `:scope > a.get`. Open the second: only its `.tile-panel` holds an `a.get`, as `lastElementChild`, with text `↓ Download`, the second volume's `href`, `download` `''` and its `aria-label`.
    - `it.each(['off', 'on-select'])`, "puts the download on the cover's corner, one per tile, in %s mode": three `li > a.get`, each with its tile's `href` and text `↓`, and no `.panel-open`. For `on-select`, reveal one: its panel holds no `a.get`.
  - "keeps the download anchor outside the tile link": the same check over the corner and the panel.
  - "shows no download at all when the account may not download" and "…when no gate sits above the screen": over each mode, with the first panel open in `button`.
  - "withholds the per-tile download…": in the panel (`button`) and on the corner (`off`).
  - "names each i 'details for' its volume": per tile, `details for Go-Toubun no Hanayome v01` and so on.
  - "Series, the edition selector › in the address": "draws every tile as with no ?edition= while the selector is off…", "keeps an edition chosen on the page in the address while the selector is switched off…" and "applies ?edition= again once the selector is switched back on" keep their expectations, now read from the corner ↓ under `panel: 'off'`. They pass before and after; under `button` they would go red.
- [ ] **Step 2: Failing tests, browser** (`Series.tile.browser.test.tsx`, mounted by `seriesPage`). Every case calls `pointerRulesAs` first.
  - (a) `it.each` over 1280 comfortable, 1280 compact and 390, "sets i 0.25rem in from the cover's bottom-right corner at %s, on a readable and an unreadable tile": per tile, `i.right = cover.right − 4` and `i.bottom = cover.bottom − 4`. Under `phone` the size is 44×44, under `mouse` at least 24. The unreadable tile is Review Focus 5.
  - (b) "draws ⇄ at the cover's top-right, clear of i": `top = cover.top + 4`, `right = cover.right − 4`, and `overlapOf` with `i`'s area is 0. A hand-mounted library `.grid` tile's ⇄ keeps `bottom = cover.bottom − 4`.
  - (c) `it.each(['off', 'on-select'])`, "puts ↓ on i's corner, at i's size, where a tile has no i (%s)": the same insets and size as (a).
  - (d) "hides i until its tile is hovered or holds focus, or its panel is open, where every pointer hovers", under `mouse`:
    - At rest, `opacity` is `'0'`, `pointerEvents` is `'none'`, and `elementFromPoint` at `i`'s middle is inside the tile's link.
    - Hovering the link gives `'1'` and `'auto'`.
    - With the pointer moved to the `h1`, Tab from the tile's link focuses `i`, at `'1'`.
    - Open the panel, then press on its alignment text so focus leaves for `<body>`, and move the pointer off: `'1'`.
  - (e) "moves no tile as i fades in": every `li`, link and `.title` rect is equal before and after hovering.
  - (f) `it.each` over `phone` (44) and `touch laptop` (24), "keeps i shown on a %s, %i px": `opacity` is `'1'` at rest, the size is as named, and `elementFromPoint` at its middle is `i`. The touch laptop is Review Focus 3.
  - (g) "fades i over 120 ms, and not under reduced motion": `transitionDuration` is `'0.12s'`, then `'0s'` under `Emulation.setEmulatedMedia` `prefers-reduced-motion: reduce` (reset in `onTestFinished`).
  - (h) "draws the panel's ↓ Download as a word button": `fontSize` is `'14px'`, `paddingRight − paddingLeft` is 4, and the height is at least 24; under `phone`, at least 44.
  - Node, in `index.css.test.ts`, "hides i only where every pointer hovers": `library.css` holds the rule under exactly `@media (hover: hover) and (not (any-pointer: coarse))`.
  - Run jsdom, node and browser: all of the above fail, but (e) and (b)'s library-grid part, which pass first and are held by their Step 4 mutations. The existing coarse cases elsewhere go red as soon as the rule lands without the helper; land both in one step.
- [ ] **Step 3: Implement**, the helper with the CSS. Run all three projects to green.
- [ ] **Step 4: Watched mutations.**
  - ↓ left under the tile in `button` mode: the button-mode case fails.
  - ↓ also in the on-select panel: the jsdom corner case's `on-select` row fails.
  - Drop the panel ↓'s guard: the may-not-download case fails.
  - `display: none` for hiding: (d)'s Tab part fails.
  - Drop the `:focus-within` arm: (d) fails. Drop the `aria-expanded` arm: (d)'s last part fails.
  - `i` in flow (`position: static`) and hidden by `display: none`: (e) fails. No single change to the specified CSS moves a tile, since `i` is absolutely positioned.
  - Drop `and (not (any-pointer: coarse))`: the node case fails.
  - Drop the reduced-motion rule: (g) fails.
  - `.cover-box .aligned` unscoped: (b)'s library-grid part fails.
  - Margin `150%` → `100%`: (a) fails.
- [ ] **Step 5:** `tsc -b`, `biome check`, coverage and floor, the comment check. Commit `… (O283)`. Write `$E/T3-report.md`.

---

### Task 4: The panel lies over the rows below (O283, O274)

Spec, "Placement" and "Contents"; gaps 7 and 8 above. No close rules yet: `i` still toggles, and Task 5 adds the rest.

**Files:**
- New `web/src/tilePanel.ts`:
  - `contentSpan(area: Element): { start: number; end: number }`: the area's content box, left to right. It is `MetadataEditor.tsx`'s deleted `contentSpan`; recover it with `git -C /home/davey/projects/taiyaku show dd70ca14:web/src/MetadataEditor.tsx`, ~227-236.
  - `panelSpan(tile: { left: number; right: number; width: number }, gap: number, area: { start: number; end: number }): { left: number; width: number }`: the rule in Global Constraints. It is the deleted `layOpen`'s width and left (~255-264), with its half-pixel reason in the comment.
  - `layPanel(panel: HTMLElement, tile: HTMLElement): void`. It finds the grid (`tile.parentElement`) and `.library > main`, and does nothing without them. It reads the column gap and the tile's rect, then sets `panel.style.left` (relative to the tile) and `panel.style.width`.
- `web/src/Series.tsx`:
  - `openTile = useRef<HTMLLIElement>(null)` on the open volume's `li`.
  - A `useLayoutEffect` on `[open, layout]` calls `layPanel` at once and from a `ResizeObserver` over the open tile alone (guarded as in Task 1). No `resize` listener and no observer on the page area: `main` has no `max-width`, so a change of the window's width (spec:100-101) changes the page area's, and the tile, `1fr` of the area, changes width with either and with the density. Its comment says so. If Biome flags `layout` as unread, a `biome-ignore lint/correctness/useExhaustiveDependencies` names it a trigger, as in Task 1.
  - The panel's children in order: `MetadataEditor` (its form, then its stamp section), `AlignmentPanel`, ↓ Download. The panel is never scrolled into view (item 6).
  - The panel `div`'s comment (~714-718, "With no margin, border or padding, it moves none of its parts") is rewritten: the block is now the drawn panel.
- `web/src/styles/library.css`:
  - `.tile-panel`: absolute at `top: calc(100% + var(--space-1)); left: 0`, z-index, look, flex column with `gap: var(--space-3)`, and `box-sizing: border-box`.
  - The pre-measure width `calc(200% + 1rem)`, and `calc(200% + 0.6rem)` under `.library.compact`.
  - `.tile-panel .alignment-panel { margin: 0; padding: 0; border: 0 }`.
  - `p` and `.stamp-comicinfo > h3` margins 0 inside it.
  - Rewrite the comment on `.alignment-panel, .metadata-editor, .stamp-comicinfo` (~441-444). The panel now lies over the rows below (O283), and its long text still has nothing to break at.
- Tests:
  - New `web/tests/tilePanel.test.tsx` (jsdom).
  - New `web/tests/Series.panel.browser.test.tsx`. It mounts with Task 3's `seriesPage`, `panel: 'button'`, admin with `can_download`, with `library.oneSeries`, `library.work`, `metadata.patchWork`, `stamp` and `libraries.deriveStatus` mocked as their current files do. It imports `oneLineIn`, `lineOf`, `pastTheEdgeOf` and `lineCount` from `tests/support/lines.ts` (Task 1).
  - `web/tests/MetadataEditor.browser.test.tsx`: three of the four cases of "a stamp plan drawn in a volume's tile (O274)" move to the new file (Step 2 names the one that goes). The describe goes, with its mount `planInTile` and with `unboundedWidthOf`, which only the dropped case used; the file's imports from `lines.ts` shrink to what remains used.
  - `web/tests/classNames.test.ts`: the `HOOKS` entry moves with them.
  - `web/tests/index.css.test.ts`: the stacking case.

**Interfaces:**
- Consumes: `.tile-panel`, its `a.get` and `seriesPage` (Task 3); the field's re-fit on a width change and `lines.ts` (Task 1).
- Produces: `contentSpan`, `panelSpan` and `layPanel` as above, and `openTile` in `Series`. Task 5 uses `openTile`.

- [ ] **Step 1: Failing tests, jsdom** (`tilePanel.test.tsx`).
  - `panelSpan`:
    - (a) "lies from the tile's left edge, two tiles and the gap wide, where that fits": tile `{100, 230, 130}`, gap 16, area `{24, 1000}` gives `{ left: 100, width: 276 }`.
    - (b) "lies back from the tile's right edge where rightward would pass the area": tile `{870, 1000, 130}` gives `{ left: 724, width: 276 }`.
    - (c) "is the area's width from either column where two tiles and the gap are wider": area `{16, 374}`, tiles `{16, 187, 171}` and `{203, 374, 171}`; both give `{ left: 16, width: 358 }`.
    - (d) "is held to the area's start when a tile sits half a pixel right of it": tile `{16.5, 187.5, 171}` gives left 16.
  - `layPanel`:
    - (e) "lays a panel from its tile, its grid's gap and its page area": `.library > main` with inline padding, the grid with inline `columnGap`, and rects stubbed with `vi.spyOn(…, 'getBoundingClientRect')` and `clientLeft`/`clientWidth` defined. With (b)'s numbers, `style.left` is `-146px` (relative to the tile) and `style.width` is `276px`.
    - (f) "lays nothing outside a page area": the style is untouched.
- [ ] **Step 2: Failing tests, browser** (`Series.panel.browser.test.tsx`). Every layout case opens a panel by hovering the tile, then a real click on its `i`, under `pointerRulesAs('mouse')`.
  - (g) "opens over the rows below, 4 px under its tile, and moves no tile", at 1280 with enough volumes for three rows, opening a first-row tile. A row lies below, and the panel intersects a tile in it (asserted first). Every tile's, link's and title's rect is equal before and after. `panel.top` is `li.bottom + 4`.
  - (h) `it.each` over comfortable and compact, "is two tiles and the gap wide, from its tile's left edge, at 1280 px (%s)".
  - (i) "lies back from its tile's right edge in the row's last column at 1280 px". The fixture tells: the tile's right edge is the area's, and its left edge plus the width passes the area. `panel.right = tile.right`, and `main.scrollWidth === main.clientWidth`.
  - (j) `it.each` over both columns, "spans the page area at 390 px from the %s column": the panel's left and right are the area's content edges, and `document.documentElement.scrollWidth ≤ innerWidth`. (At 390 px the pre-measure width is already the final one, so no mutation of the field's re-fit could show here; (m) holds it.)
  - (k) "keeps a five-line field and its own width for a title with no space, longer than five lines" (Review Focus 1): a 300-character title of word characters. The panel is as wide as in (h), `pastTheEdgeOf(panel)` is `[]`, the field's content height is 5 × line-height, and its `scrollHeight > clientHeight`.
  - (l) "is laid again when the window's width changes": open the last column at 1280, then narrow the window (`page.viewport(w, 800)`, `w` under 900) to a width where the tile is no longer last and has room to its right. Asserted first: it is not last, and its left edge plus the panel's new width ends at least 8 px inside the area, so rightward does not hold only by equality. Then the panel's left is the tile's left, and it is within the area.
  - (m) "is laid again, its field fitted again, when the page area's width changes and the window's does not": the sidebar column is widened, as the deleted O274 case did. The title is one whose `lineCount` differs between the field's content widths before and after, asserted first. Then the panel is two tiles and the gap wide at the new tile width, and the field's height is the count after.
  - (l) and (m) read once the observers have run (`expect.poll`), never a fixed frame after the change.
  - (n) "draws over the covers and the marks of the row below": a lower tile is `aligned`, and `i` is shown under `touch laptop`. `elementFromPoint` at a point on that tile's ⇄, and at one on its `i`, within the panel, lands inside `.tile-panel`.
  - (o) "is drawn as View's panel is": `padding`, the `border-*` triplets, `backgroundColor` and `boxShadow` equal the page's own `.view-panel`'s computed values (its element is in the DOM while hidden, and `seriesPage`'s `BrowseControlsContext` is what draws it).
  - (p) "holds the Title field, the stamp section, the alignment detail and ↓ Download, top to bottom, 12 px apart, the alignment detail bare": with `can_stamp`, the children's classes in order. Each gap, next top less previous bottom, is 12. `.alignment-panel`'s `borderTopWidth`, `paddingTop` and `marginTop` are all `0px`.
  - The moved O274 cases, renamed "…stays inside the panel…", now open a real panel. "The editor stays inside a right-hand tile at 390 px…" asserts `pastTheEdgeOf(panel)` is `[]`, and so on. "The title box keeps to its tile's column at 1280 px" goes; (h) and (k) hold its claim.
  - Node, `index.css.test.ts`, "draws an open volume's panel above the covers and under the phone bar and View": the `z-index` of `.tile-panel` is a number greater than 0, less than `.phone-bar`'s and less than `.view-panel`'s.
  - Run jsdom, node and browser: all fail but the moved cases, which may pass first (an in-flow panel wraps inside its tile); their Step 4 mutation shows them.
- [ ] **Step 3: Implement**; run all three projects to green.
- [ ] **Step 4: Watched mutations.**
  - Drop `panelSpan`'s clamp: (d) fails. ((j)'s right column cannot show it: at 390 px its leftward start is the area's start with or without the clamp.)
  - Always rightward: (b) and (i) fail.
  - `.tile-panel` static: (g) fails.
  - Drop the tile observer, the one re-lay trigger: (l) and (m) fail, since nothing else lays the panel again.
  - Drop the z-index: (n) fails.
  - Alignment before the editor: (p) fails.
  - Drop the field's observer (Task 1's): (m) fails. This checks gap 8's "the field's width observer catches the rest".
  - Drop `overflow-wrap: anywhere` from `.alignment-panel, .metadata-editor, .stamp-comicinfo`: the moved cases fail.
- [ ] **Step 5:** `tsc -b`, `biome check`, coverage and floor (`Series.tsx` and `tilePanel.ts` named in the floor's output), the comment check. Commit `… (O283, O274)`. Write `$E/T4-report.md`.

---

### Task 5: Closing the panel loses nothing (O283)

Spec, "Closing", "`on-select` mode", settled items 2, 3 and 6; gaps 5, 6, 12 and 13 above.

**Files:**
- `web/src/Series.tsx`:
  - `editor = useRef<TitleFlush>(null)`, passed as `ref={editor}` to `MetadataEditor`.
  - `choosePanel` (below): the only writer of `open`. `reveal` keeps its fetch and calls it. `i`'s toggle calls `choosePanel(undefined)` or `reveal`.
  - `refocusing = useRef(false)`: true only for the synchronous span of the `focus()` that returns focus after a close. `onFocusCapture` reveals nothing while it is true.
  - One `useEffect`, while a panel is open, registers document listeners:
    - `keydown` in the capture phase. On Escape, unless it ends a composition (`isComposing` or `keyCode === 229`) or its target is `textarea[name='title'][data-editing]` (that Escape is the field's), it calls `stopPropagation` and `choosePanel(undefined, back)`.
    - `pointerdown` outside `openTile`: `choosePanel(undefined)`, unless its target is the scroller itself, the page area (`.library > main`) or the root element (gap 12).
    - In `button` mode only, `focusin` outside `openTile`: `choosePanel(undefined)`.
  - `back` focuses, under `refocusing`, a target that depends on the mode. In `button`, the tile's `.panel-open`. In on-select, its link; else its corner `a.get`; else its `.tile-unreadable` box, which carries `tabIndex={-1}` in on-select mode only, so it takes focus without becoming a Tab stop.
  - The comment on the on-select hover guard (~617-623, "(O274)") is rewritten for what remains true.
- Tests: `web/tests/Series.test.tsx`, `web/tests/Series.onSelect.browser.test.tsx`, `web/tests/Series.panel.browser.test.tsx`.

**Interfaces:**
- Consumes: `TitleFlush` and the `ref` prop (Task 2b); `data-editing` (Task 1, kept by Task 2a); `openTile` and `openPanel` (Task 4, and as now).
- Produces, in `Series`, for the rest of the file:

```ts
/**
 * Every change of which volume's panel is open: the open editor's `flush`
 * first, asked again after each `true` (text typed while a save was out),
 * then the change once it answers `null`; none on `false`, the failure then
 * shown in the panel still open. A call made while one waits replaces it.
 * `after` runs once the change is made.
 */
const choosePanel = (next: string | undefined, after?: () => void): void
```

- [ ] **Step 1: Failing tests, jsdom.** New describe "Series, closing a volume's panel (O283)" in `Series.test.tsx`. Its mount puts the page in `.library > main` and wraps it in `BrowseControlsContext.Provider value={{ change: () => {}, reset: () => {} }}`, as "Series, its View panel — spec §2" does, so `.view-button` is drawn for (j). Presses are `new MouseEvent('pointerdown', { bubbles: true })`, as `Sidebar.test.tsx` sends them.
  - (a) "closes on Escape and sends focus back to i": focus the panel's ↓ Download, then Escape. No `.tile-panel` remains, and `document.activeElement` is the tile's `.panel-open`.
  - (b) "puts the title back on an Escape in the Title field, and closes on the next": after the first Escape the panel is open, the value restored and focus in the field. After the second, it is closed and focus is on `i`.
  - (c) "stops the Escape that closes it on the way down": a bubble-phase `keydown` listener on `document` sees nothing. Its pair: with no panel open, the same Escape reaches it.
  - (d) "leaves an Escape that ends a composition alone": `isComposing: true`; the panel stays.
  - (e) "closes on a press outside its tile and panel, and not on one inside".
  - (f) "closes when focus arrives outside it in button mode, and not in on-select".
  - (g) "closes an on-select panel on Escape with focus back on the tile's link, and leaves it closed". Focus the Title field and press Escape (the field's own: the panel stays). Asserted first: `document.activeElement` is inside `.tile-panel`. The next Escape closes the panel, focus is on the tile's link, and no panel is open after.
  - (h) "closes an unreadable tile's on-select panel on Escape with focus still in the page" (Review Focus 5): the tile has no link and no ↓. `document.activeElement` is its `.tile-unreadable`, and no panel is open.
  - (i) "keeps an on-select panel when the pointer leaves its tile" (`mouseout` from its link to the page).
  - (j) `it.each` over five closes, "saves a changed title before the panel closes, on %s". The closes: Escape after focus moves to ↓ Download; a press outside with focus still in the field; a click on `i`; focus arriving on `.view-button`; a click on the second tile's `i`. In each, type into the field, then hold the patch on a `deferred`. While it is out, the first panel is still drawn and the call is `['gotoubun-v01', { title: renamed }]`, made once. Once it resolves, the end state is reached (closed, or the second panel open), still with one call.
  - (k) "keeps the panel open, its failure under the field, when the save a close needs fails": a press outside, then a rejection. The panel is drawn and `p.failure` reads the message.
  - (l) "moves an on-select panel only once the save out has landed": type, then blur; the save is deferred. A `mouseover` on the second tile's link leaves the first panel; on resolve, the second opens.
  - (s) "keeps the panel through a press on the page area itself or the root, and closes on one on the page's heading" (gap 12): presses whose target is `main`, then `document.documentElement`, leave the panel; one on the `h1` closes it.
  - (t) "saves what is typed while a close's save is out before it closes" (gap 12): type `A`, press outside, the save deferred; with focus still in the field, type `AB`. Resolve the first: a second call sends `AB`, and the panel is still drawn. Resolve the second: it closes, two calls in all.
- [ ] **Step 2: Failing tests, browser.**
  - In `Series.panel.browser.test.tsx`:
    - (m) "sends one save and closes once it lands, for a real press outside while the field's own save is out", plus a variant where it fails (Review Focus 2). Click into the field and type. The patch is deferred. A real click on the `h1` (pointerdown before the field's blur) leaves the panel drawn with one call. On resolve it is closed, still one call. In the failing variant, it stays drawn with the failure.
    - (n) "closes on real Escapes: the first restores the title, the second closes and focuses i", under `mouse`, with real keys, also passing alone with `-t`.
    - (o) "a real press on another tile's i saves the typed title, then opens that tile's panel".
  - In `Series.onSelect.browser.test.tsx`, the volumes grow to fill two rows at 1280, and the fixture asserts a tile lies under the first panel:
    - (p) "keeps the panel, the focus and the restored title on the first Escape in the Title field, with the pointer resting on the panel over the next row". The pointer's point is in the panel and within a lower tile's rect, asserted first.
    - (q) "saves the typed title, then moves the panel, on Tab from the Title field to the next tile's link", with `metadata.patchWork` mocked and deferred.
    - (r) "reveals the volume under the pointer again once a press outside has closed the panel": the old "…once focus has left the open panel". The `h1` click now closes it (`editorIn` all false) before the hover.
  - Run jsdom and browser: all fail. (i) and the second half of (f) pass already and stay as guards.
- [ ] **Step 3: Implement**; run node, jsdom and browser to green.
- [ ] **Step 4: Watched mutations.**
  - Each close calls `setOpen` directly: (j)'s press-outside and `i` rows, and (o), fail.
  - Close on a failed flush: (k) and (m)'s failing variant fail.
  - Drop `stopPropagation`: (c) fails.
  - Close on the field's first Escape: (b) and (n) fail.
  - Drop `refocusing`: (g) fails.
  - `focusin` in both modes: (f)'s second half fails.
  - Drop the fallback target: (h) fails.
  - Keep the first waiting call rather than the newest: (o) fails, the second panel never opening.
  - Close on every press outside the tile, the page area's and the root's included: (s) fails.
  - Close once `flush` answers `true`, without asking it again: (t) fails.
- [ ] **Step 5:** `tsc -b`, `biome check`, coverage and floor, the comment check. Commit `… (O283)`. Write `$E/T5-report.md`.

---

### Task 6: A panel past the page's end moves no tile (O283)

Spec, "The scrollbar gutter".

**Files:**
- `web/src/styles/library.css`: the two gutter rules (Global Constraints). The comment says why: a panel running past the end of a page that did not scroll adds a classic scrollbar, which narrows the grid. A pair of its own, not added to the selector lists of `settings.css`'s `.library > main:has(.config, .split)` pair (~1034-1042): that pair's reason is the Configuration split's breakpoint, and each stays with its screen's styles.
- New `web/tests/Series.scrollbars.browser.test.tsx`, which runs in the launch that shows scrollbars. It mounts with Task 3's `seriesPage` and gives the host the window's height (`host.style.height = '100%'`, as `Cover.browser.test.tsx`'s `at` does).

- [ ] **Step 1: Failing tests.**
  - (a) `it.each` over a one-volume series and the last row's first tile of a full grid, "keeps every tile in place as a panel runs past the end of a page area that did not scroll, at 1280 px". The window's height is named in the case, low enough for the first assertion (1280 × 800 is probably too tall for one volume). Before opening, `main.scrollHeight ≤ main.clientHeight`; after, it scrolls (asserted first). Every tile's `left` and `width` are unchanged.
  - (b) The same at 390 px, where the document scrolls, not `main`: before opening, `document.documentElement.scrollHeight ≤ innerHeight`; after, it is greater (asserted first).
  - (c) "scrolls the open panel's foot into view": scroll `main` to its end; the ↓ Download's bottom is at or above `main`'s visible bottom.
  - (d) "reserves no gutter where the page draws no volume grid": the table layout. `getComputedStyle(main).scrollbarGutter` is `'auto'`, and at 390 the root's is too.
  - (e) "keeps the panel open through a real press on main's scrollbar" (gap 12): with (a)'s one volume open, a real mouse press on `main`'s scrollbar track, less than the scrollbar's width in from `main`'s right edge. The panel is still drawn.
  - Run browser: (a) and (b) fail. (e) passes first, Task 5's check being in.
- [ ] **Step 2: Implement**; run browser to green.
- [ ] **Step 3: Watched mutations.** Drop the `main` rule: (a) fails. Drop the root rule: (b) fails. Unscope it to `.library > main`: (d) fails. Drop Task 5's scroller check: (e) fails; if it cannot go red (Chromium sends the page no `pointerdown` for a scrollbar press), delete (e), say so in the report, and leave the rule to Task 5's (s).
- [ ] **Step 4:** `tsc -b`, `biome check`, the whole browser project (both launches), the comment check. Commit `… (O283)`. Write `$E/T6-report.md`.

---

### Task 7: The ledger and the specs (O274, O283, O285)

Spec, "Status-ledger effects" and "Rulings this reverses or bends". Status dates are 2026-11-14. Every figure is measured or derived from the table, never added to the old one.

- [ ] **Step 1: Close O283 and O274** with the five-part edit in `docs/status.md`:
  1. Each row's checkbox is `[x]`.
  2. Each row's text says what was done, by symbol, without line numbers, beginning `**Closed 2026-11-14 by the compact series tiles: …**`.
     - O283: `i` on the cover and its hover rule, ⇄ at the top-right, ↓ in the panel's foot or on `i`'s corner, the panel over the rows below (`tilePanel.ts`), the Title field saving as it is left, and the closes.
     - O274 (~1899): the panel, stamp section included, holds the whole editor two tiles wide over the rows below in `button` and `on-select` mode alike. The 1280 px column it named is gone.
     - Rename a site whose symbol this branch deleted (`layOpen`).
  3. Bullets.
     - Rewrite O274's (~4643, with lane 3's) to "… Closed."
     - After the last 2026-11-14 section, "**Shorter book URLs of 2026-11-14**" (~4811-4826), and before "## What comes next", add an intro, "**The compact series tiles of 2026-11-14** (`docs/superpowers/specs/2026-11-14-compact-series-tiles-design.md`) closed O274 and O283 and opened O285", followed by O283's bullet and O285's.
     - O283's bullet records what the spec reopened and reversed: the 2026-09-18 settlement (style-pass §7 ②), kept by the phone fixes' non-goal; the owner's Title box ruling of 2026-11-13 (one line at rest, opening on focus, Enter as Save, unsaved text kept); and Download §8's one control per tile, still one per tile.
  4. The test counts at ~183. Run node, jsdom and the whole browser project with `--reporter=default`, and set the four web figures from their summary lines: `node`, `jsdom`, their total, and the browser project's. The web figures have no gate behind them; the Rust figure does not move.
  5. The "… are now closed — …, leaving …" sentence (~1498), with O274 and O283 inserted in order, and "What remains: the … open items" (~5349). Its count word wraps, so grep the opener and read the next line.
- [ ] **Step 2: Open O285.** Its row goes after O284's (~1909), in the table's shape: a cover-size slider in place of `density`, whose compact sets `.library.compact .grid` and `.volumes` to `minmax(6.5rem, 1fr)` against `.volumes`' 7.5rem. The site is `browseModule.ts`'s `density`, `Shell.tsx`'s `compact` class and `library.css`'s `.library.compact` rule; opened by the owner, 2026-11-14. Its bullet goes in Step 1's section. The section head's count word (~1463, "Two hundred and eighty-four, each found by…", O284 included since #247) counts the rows, and becomes eighty-five.
- [ ] **Step 3: Pointers.** Under `2026-09-17-style-pass-design.md` §7 (~457-466), and in `2026-11-13-phone-fixes-design.md` at its Non-goals (~29-32) and its Title box section (~139-160), add an italic line beginning `*Reopened 2026-11-14 by the owner:*` that names `2026-11-14-compact-series-tiles-design.md`. Their prose is kept. In `docs/status.md`, the style pass's text-size sentence (~6833, "the one-glyph tile controls `a.get` and `.panel-open` 12px") goes partly false: it gains the panel's ↓ Download as a 14 px word button (O283), the corner ↓ and `.panel-open` staying 12 px.
- [ ] **Step 4: Sweep.**
  - `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`, checked against the three sentences.
  - Every table row holds exactly six `|`.
  - `rg -n 'O274|O283|O285|layOpen|settlement|one-glyph' docs web/src web/tests`: read every hit. Prose that speaks of one as open is corrected, or marked if it is history.
- [ ] **Step 5:** Commit `Close O274 and O283, open O285 in status.md`. Write `$E/T7-report.md`.

---

### Finish

1. **Whole-branch review** of `main..HEAD`, in two lanes: (a) behaviour, Tasks 2a, 2b and 5; (b) layout, CSS and docs, Tasks 1, 3, 4, 6 and 7. Each reviews against the spec, the gaps above and the Review Focus. Fix rounds go back to the same implementer by SendMessage. After a fix, grep for the finding's shape, and review the fixes too. If a fix adds tests after Task 7, measure the counts again.
2. **Gate** on the final commit, by the Gate line. Read `$E/gate.exit`.
3. **The drive**, by the UI-prototype recipe (a scratch copy of `/home/davey/temp/manga-test`, `local`/`pw`, the port checked free before and after, the library's sha256 and a marker before and after). Drive it with a real mouse at 1280 and 390, and under DevTools' touch emulation:
   - `i` fades on hover and stays under touch.
   - The panel lies over the rows below, from the last column too.
   - A title saves as the field is left, and `Saved` comes and goes with nothing moving.
   - Escape, an outside press and `i` close the panel, and none loses a typed title.
   - On-select and `off` show ↓ on the corner.
   - Opening the last row's panel adds a scrollbar and moves no tile.
   Stop the server by PID, and delete the scratch copy in its own Bash call by its literal path.
4. **Claim check** of the status edits, the spec pointers and the PR body. Put the PR body in pass 1; scope later passes to the correction commit.
5. **The PR body** at `$E/pr-body.md`, its title the first line, measured under 255 characters. It names the spec gaps above. Ask the owner before pushing.

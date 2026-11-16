# Cover size and the Continue row Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hold the owner-approved prototype on `continue-align` (the Cover size slider in place of `density`, exact-size covers, and the Continue row's heading, columns, fade and line) with tests watched failing against hand-applied defects, fix what the tests and a critical read found, and record it in the docs (closes O285, opens and closes O288).

**Architecture:** The prototype is already on the branch (`git diff 2125f347 e77c1803 -- web`); two owner rulings of 2026-11-15 came after it (spec items 9 and 13) and are built here, not in it. The slider stores one number, `coverSize`; `Shell.tsx` sets it as the bare `--cover-n` on `.library`, and `library.css` derives the track (`--tile-min`), the least gap (`--tile-gap`) and which layout is in force (`--tile-fill`) for the grid, the Continue row and a series page's volumes: exact covers with the spare room in the gaps from 900 px up, `minmax(size, 1fr)` columns at the least gap below. The slider is a kind of setting: `browseModule` declares `coverSize` a slider on its scale (`ReaderModule.sliders`), `settingsPanel.ts`'s `fieldsOf` infers the kind and `settingsFields.tsx`'s `FieldControl` draws it, so no panel names a key. One hook, `useContinueRow`, gives the row the grid's spread gap (900 px up) or its filled column width (below) and marks the sides with more cards; `tilePanel.ts` reads the gap the columns are actually laid apart by and the tile's own width. No new dependency.

**Tech Stack:** React 19, TypeScript, plain CSS (`@property`, `mask-image`, `:has()`), Vitest 4.1 (`node`, `jsdom` and a Chromium `browser` project), Playwright (Chromium and Firefox) for the drive.

**Spec:** `docs/superpowers/specs/2026-11-14-cover-size-and-continue-row-design.md` (approved by the owner 2026-11-14/15). Read it whole before any task; item numbers below ("item 6") are its.

## Global Constraints

- Cover size: 96 to 240 px; default 144 px at the middle of the track; left half 96–144 in 1 px steps, right half 144–240 in 2 px steps, 97 positions; an arrow key moves one position (item 6).
- The mark: 1 px wide, 6 px tall, `--line-strong`, under the track where 144 sits, showing with the thumb on it (item 7).
- A double-click anywhere on the slider resets to 144 and saves like any change; no catch at the middle (item 8).
- From 900 px up, covers exactly the slider's size; as many columns as fit at the least gap, the spare room spread into the gaps; a short last row on the same columns; least gap 0.6rem at 96 px rising to 1rem at 144 px, 1rem above (item 9).
- Below the shell's 900 px breakpoint (`@media (width < 900px)`, the drawer's), the columns are `minmax(size, 1fr)` at the least gap: as many as fit, each grown to fill the row, so the slider sets the least a cover may be (item 9, ruled 2026-11-15). Across the breakpoint nothing a script set for the other layout is left in force.
- The slider is a declared kind (item 13, ruled 2026-11-15): a module's `sliders` names a number preference's `SliderScale`; `FieldControl` draws it in the View panel and the reader's `Settings.tsx` alike; `ViewMenu.tsx`, `Settings.tsx` and `settingsFields.tsx` name no preference key; C1 §4's tests (`ViewMenu.test.tsx` ~60 and the doMock case ~104) hold whole. The mark sits at the track's middle, so a declared slider's default must be its middle position, which `buildRegistry` checks at import.
- Stored `compact` becomes 104 px, `comfortable` 144 px; the next save stores the size and drops `density`; a stored size off the scale is drawn at the nearest position's size (item 10).
- Continue heading: its rule and the sidebar's Libraries rule on the same pixel row at ≥ 900 px (fine and coarse pointer); below 900 px unchanged; nothing above the Continue heading moves (items 1, Constraints).
- Continue covers are the grid's width and sit over the grid's columns, also when the row scrolls (item 2), in both layouts: from 900 px up the size at the grid's spread gap, below it the grid's filled column width at the least gap (Constraints); the gap or column is in place before first paint; no React render per scroll frame or resize.
- O283's tile panel spans the gap the columns are laid apart by and the tile's own width, in both layouts (Constraints).
- Fade: a mask, 80 px on each side the row can still scroll to, clear for the last 12 px, half see-through at 50 px; none when the row fits; lifted while a card has `:focus-visible`; a covered card still takes a press (item 3, Constraints).
- The line: `--line`, 1 px, under the row across its width, 16 px above it and 16 px below it to the chips; drawn only while the row is (not with the strip off, on search results, or with nothing in progress) (item 4).
- The sizes stay in CSS: the slider sets only `--cover-n`; a script reads `--tile-min`, `--tile-gap` and `--tile-fill` from the live computed style, so the breakpoint is written once, in `library.css`.
- View menu: its words cannot be selected; a checkbox's whole row, to the panel's edges, toggles it with a pointer cursor, in Firefox as in Chromium (items 11, 12).
- Firefox is the owner's browser; it has no test project, so every item is also driven in Firefox (Task 11).
- A code comment says why; no history, no measurements (`CLAUDE.md`). `scripts/check-comments.mjs` caps a comment block at 8 counted lines; it is **red at the prototype** (5 blocks, listed in the tasks that own them).

## Review Focus

1. **A window resize with a volume's panel open.** From 900 px up covers no longer change width with the area, so the observer on the tile no longer fires; the panel must be laid again anyway, and again when the window crosses 900 px and the tile turns from the size to the filled column (Task 5, fix and test).
2. **A larger root font size.** The least gap is in rem, the covers in px; the Continue row's columns and the heading's alignment must still hold at a 20 px root (Tasks 6 and 8, one case each).
3. **A cover size change while the Continue row is scrolled.** The fade's sides and the columns must follow a slider change at any scroll position (Task 7, one case).
4. **A cover drawn wider than about 170 px.** `Cover.tsx` offers 256/512/768 px long edges by screen density only, sized for a 256 px-tall tile; a 240 px cover is 360 px tall, so the 1× pick is drawn about 1.4× up. Below 900 px a cover is filled wider than the size, up to the whole row at one column (358 px at 390 px), so a `sizes` of the size alone under-asks there (Task 9, fix and test).
5. **A coarse pointer on the slider.** The coarse block gives the range input a 44 px floor, and the prototype's mark sits under the 44 px box, not under the drawn track; the slider is now drawn in the reader's panel too, so the fix is the kind's, not the View panel's (Task 2b, fix and test).

---

## Before any task

- **Where.** Worktree `/home/davey/projects/taiyaku/.claude/worktrees/continue-align` (below `$W`), branch `continue-align`. Absolute paths or `git -C`/`env -C`; never `cd <dir> && …` (`cd` is zoxide). Bash is zsh with `noclobber`: start a log with `>|`. Never `pnpm exec`/`pnpm install` in the worktree; call `$W/web/node_modules/.bin/<tool>` through `env -C $W/web` (biome finds the main checkout's config otherwise). `$W/web/node_modules` is a real directory; keep it so.
- **`/tmp` must have room.** On 2026-11-15 `/tmp` (tmpfs) was full, and every vitest browser run, Playwright launch, the drive's build and the lane server's cover cache need it. Check `df -h /tmp` before Task 1 and stop if it is full; do not delete anything there that this lane did not write.
- **Evidence** goes in `/home/davey/projects/taiyaku/.superpowers/sdd/continue-row-evidence/` (below `$E`; gitignored, outside the worktree, which a finished agent's cleanup can delete; `target/` is not durable). Each task writes `T<n>-report.md` there by Bash, with its logs.
- **Commands.** Whole projects, into logs, the exit status read from the command, never through `tail`:
  - `env -C $W/web ./node_modules/.bin/vitest run --project node --project jsdom --reporter=default >| $E/T<n>-nj.log 2>&1; echo $? >| $E/T<n>-nj.exit`
  - the same with `--project browser` (both Chromium launches) into `T<n>-browser.log`.
  - A named file or `-t` only to watch a red, never as a task's check; a real-key test is also run alone with `-t` (the frame must be focused first: focus an element in the page before the first key).
- **Watched defects.** Every test this plan names has its defect named beside it. Apply it by hand, run the test, see it fail for that reason, restore the code exactly (`git diff` empty for that file), see it pass. A test that passes against its own defect is not kept as proof: report it and strengthen it. The report lists each defect, the red line and the restore.
- **Comparisons are against main, never against a list.** The base is `origin/main` at 2125f347 (#251, docs-only; its `web/` is 7c9a3773's).

---

### Task 1: Rebase, the baseline, the scale and the carry-over, and the suites the prototype broke

**Files:**
- Modify: `web/src/browseModule.ts` (the scale's doc, `loadBrowsePrefs`)
- Modify: `web/tests/browseModule.test.ts`, `web/tests/Shell.test.tsx` (the two `density` cases, ~741 and ~870), `web/tests/ViewMenu.test.tsx` (~60), `web/tests/Library.test.tsx` and `web/tests/Series.test.tsx` (the "holds … Density and Reset" cases), `web/tests/support/seriesPage.tsx` (`compact` → `coverSize`) and its callers in `web/tests/Series.panel.browser.test.tsx` and `web/tests/Series.tile.browser.test.tsx` (the parameter only; their expectations are Tasks 4 and 5's)

**Interfaces:**
- Produces: `coverSizeAt(p: number): number`, `coverPositionOf(size: number): number`, `coverSizeOf(prefs: BrowsePrefs): number`, `loadBrowsePrefs(id: string, defaults: object): object`, constants `COVER_MIN` 96, `COVER_DEFAULT` 144, `COVER_MAX` 240, `COVER_MIDDLE` 48, `COVER_LAST` 96 (all as on the branch); `seriesPage({ width, height?, panel, layout?, coverSize?, who })`, where `coverSize` sets `--cover-n` on the fixture's `.library` and is unset by default.

- [ ] **Step 1: Rebase only if main moved.** The branch was rebased onto 2125f347 (#251's O289) on 2026-11-15. `git -C $W fetch`, then `git -C $W merge-base --is-ancestor origin/main HEAD`: exit 0 is a no-op, nothing to do. Otherwise `git -C $W rebase origin/main`, stop and report on a conflict, and use the new `origin/main` as the base in Step 2 and everywhere this plan says 2125f347. Docs counts in Task 10 are computed from this `docs/status.md`, never from today's.
- [ ] **Step 2: The baseline.** `git -C /home/davey/projects/taiyaku worktree add --detach /home/davey/projects/taiyaku/.claude/worktrees/continue-align-base origin/main`; copy (`cp -a`, never a symlink) `$W/web/node_modules` into it. Run node, jsdom and browser there and on the branch, each with `--reporter=default --reporter=json --outputFile.json=$E/T1-<base|branch>-<project>.json`. List every test that passes on the base and fails or is absent on the branch (`$E/T1-broken.txt`). On 2026-11-15 node+jsdom on the branch failed 8 (all `density`); the browser project could not be run (`/tmp` full), so its list is yours to establish. Assign each broken test in the report to the task that owns it (layout and gap expectations to Tasks 4 and 5); a test failing on both sides is pre-existing and is reported, not fixed. Keep the base worktree until Task 11.
- [ ] **Step 3: Write the scale tests** in `browseModule.test.ts`:
  - `it('runs 96 to 144 in 1 px and 144 to 240 in 2 px, 144 at the middle')`: `coverSizeAt(0)` 96, `(47)` 143, `(48)` 144, `(49)` 146, `(96)` 240; for every `p` in 0..96, `coverPositionOf(coverSizeAt(p)) === p`. Defect: `2 * (p - COVER_MIDDLE)` → `(p - COVER_MIDDLE)`.
  - `it('draws a size off the scale at the nearest position')`: `coverPositionOf(104)` 8; `coverSizeOf` of 151 → 152, 153 → 154, 239 → 240, 50 → 96, 1000 → 240, −5 → 96, `NaN`, `Infinity` and `'144'` (cast) → 144. Defect: drop the clamp in `coverPositionOf`.
  - The three broken cases rewritten for `coverSize` (defaults hold `coverSize: 144`; fields `['coverSize', 'number']`, which Task 2a turns to `'slider'`; options have no `density`).
- [ ] **Step 4: Write the carry-over tests** in `Shell.test.tsx`, replacing "marks the root compact…": `it.each` over stored `{density:'compact'}` → 104, `{density:'comfortable'}` → 144, nothing stored → 144, `{coverSize: 200}` → 200, `{coverSize: 200, density:'compact'}` → 200, `{coverSize: 151}` → 152; each asserts the `.library` element's `style.getPropertyValue('--cover-n')` and that after mount the stored slice has that `coverSize` and no `density` key. Defects: `104` → `112`; let `density` win over a stored `coverSize`; keep `density` in the returned slice.
- [ ] **Step 5: Clean the prototype's history out of `browseModule.ts`.** The scale's doc gives the reason for the two steps (144 at the middle with even steps, and 104 and 144, the two sizes `density` carries over to, land exactly), not "the earlier 4 px scale"; `loadBrowsePrefs` drops only `density` (`coverFit` was stored only on the prototype's own origin, 127.0.0.1:4398, and its doc line goes with it).
- [ ] **Step 6: The rest of the broken node/jsdom cases.** "Reset from the library page" reads the Cover size slider's value (position 48, output "144 px") where it read Density's select; the three "holds …" cases name "Cover size" in place of "Density" (sentence case), and ViewMenu's count case expects 5 labels, 2 selects and 1 range. Rename `seriesPage`'s `compact` to `coverSize` and its callers (`compact: true` → `coverSize: 104`), expectations untouched.
- [ ] **Step 7: Check.** node+jsdom whole: 0 failed. `env -C $W/web ./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`: exit 0. Commit: "The cover-size scale and the carry-over from density held by tests, and the suites that named density moved to the cover size".

### Task 2a: The slider kind (items 5–8 and 13: declared by the module, inferred, drawn in both panels)

The prototype draws the slider by the key `coverSize` in `ViewMenu.tsx`. The owner ruled (2026-11-15) for a kind of setting instead, so C1 §4 holds whole. The declaration follows `options`: a record on the module keyed by preference, read by `fieldsOf`, with the default's own type as the gate (`options` only turns a string into a choice; `sliders` only turns a number into a slider).

**Files:**
- Modify: `web/src/settingsPanel.ts` (`SliderScale`, `FieldKind`, `Field`, `fieldsOf`), `web/src/module.ts` (`ReaderModule.sliders`, `sliderFaults`, `buildRegistry`), `web/src/browseModule.ts` (`coverSizeScale`, `browseModule.sliders`, the header's list of what is inferred), `web/src/settingsFields.tsx` (the slider and its header), `web/src/ViewMenu.tsx` (delete `CoverSizeControl` and the `coverSize` branch; pass `browseModule.sliders`), `web/src/Settings.tsx` (pass `mod.sliders`), `web/src/styles/shell.css` (rename `.view-cover-size`, `.view-cover-head`, `.view-cover-track` to `.field-slider`, `.field-slider-head`, `.field-slider-track`, the coarse block's selector too; none scoped under `.view-panel`, since the reader's panel draws the same markup)
- Create: `web/tests/support/sliderScale.ts`
- Test: `web/tests/settingsPanel.test.ts`, `web/tests/module.test.ts`, `web/tests/browseModule.test.ts`, `web/tests/settingsFields.test.tsx`, `web/tests/ViewMenu.test.tsx`, `web/tests/Settings.test.tsx`

**Interfaces:**
- Consumes: Task 1's `coverSizeAt`, `coverPositionOf`, `COVER_LAST`.
- Produces, in `settingsPanel.ts` (its own file imports nothing, and `module.ts` takes the type from it, as it takes `withFieldValue`):

```ts
/** A number preference drawn as a slider over positions 0 to `last`. */
export interface SliderScale {
  /** The last position; the input steps by 1, and the default sits at `last / 2`. */
  last: number
  /** The value at a position. */
  valueAt: (position: number) => number
  /** The nearest position to a value, held to 0 to `last`. */
  positionOf: (value: number) => number
  /** The value's unit as written beside the name: `144 px`. */
  unit: string
  /** The unit as read aloud, for `aria-valuetext`: `144 pixels`. */
  spokenUnit: string
}
export type FieldKind = 'boolean' | 'number' | 'string' | 'select' | 'slider' | 'object'
export interface Field {
  key: string
  kind: FieldKind
  options?: string[]
  /** A `'slider'`'s scale, as its module declared it. */
  scale?: SliderScale
  /** A `'slider'`'s default: under the mark, and what a double-click restores. */
  initial?: number
}
export function fieldsOf<P extends object>(
  defaults: P,
  options: Record<string, string[]> = {},
  sliders: Record<string, SliderScale> = {},
): Field[]
```

  `fieldsOf` gives a number default with a declared scale `{ key, kind: 'slider', scale, initial: <that default> }`; a scale declared against anything else leaves the inferred kind.
- In `module.ts`: `ReaderModule<P>.sliders?: Record<string, SliderScale>`; `sliderFaults(modules: ReaderModule<unknown>[]): string[]`, `` `<id>.<key>` `` for each declared slider whose default is not a finite number at position `last / 2` (`scale.positionOf(default) * 2 !== scale.last`), which takes in a key missing from `defaults`; `buildRegistry` throws `` a slider's default is not at its middle: `fx.early` `` (names joined by `, `) after its two existing checks. The mark is drawn at the track's middle, which is the default's place only if this holds; the browser's thumb width, which differs between engines, would be needed to place it anywhere else.
- In `browseModule.ts`: `export const coverSizeScale: SliderScale = { last: COVER_LAST, valueAt: coverSizeAt, positionOf: coverPositionOf, unit: 'px', spokenUnit: 'pixels' }` and `browseModule.sliders = { coverSize: coverSizeScale }`. `coverSizeOf` stays (Shell's `--cover-n`, Task 9's `sizes`).
- In `settingsFields.tsx`: `FieldControl` draws `kind: 'slider'` through an unexported `SliderField` (its own component, for `useId`): `div.field-slider` > `div.field-slider-head` (the caller's `rowStyle`; a `label` naming the range by `htmlFor`, then an `output` reading `` `${shown} ${unit}` ``), then `div.field-slider-track` > `input type="range"` (`min` 0, `max` `last`, `step` 1, `value` `positionOf(shown)`, `aria-valuetext` `` `${shown} ${spokenUnit}` ``; an input reports `valueAt(position)`, a number, which `withFieldValue` takes as it is; `onDoubleClick` reports `initial`). `shown` is `valueAt(positionOf(value))` for a finite number and `initial` for anything else (a stale-typed stored value, as the other kinds blank theirs).
- `web/tests/support/sliderScale.ts`: `MS_SCALE: SliderScale`, positions 0 to 10, `valueAt(p)` = `50 + 10p`, `positionOf` its inverse rounded and held, `unit` `ms`, `spokenUnit` `milliseconds`; a default of 100 is its middle.

- [ ] **Step 1: Pure tests** (node):
  - `settingsPanel.test.ts`, beside the `options` cases: `it('infers a slider for a number the module declared a scale for, with its default')`: `fieldsOf({ delay: 100, label: 'x' }, {}, { delay: MS_SCALE })` equals `[{ key: 'delay', kind: 'slider', scale: MS_SCALE, initial: 100 }, { key: 'label', kind: 'string' }]`. Defect: `fieldsOf` ignores `sliders`. `it('ignores a scale declared against something that is not a number')`: `{ on: true, label: 'x' }` with both declared → `boolean`, `string`. Defect: drop the number check.
  - `module.test.ts`: `it('names a slider whose default is off its middle or is no number')`: module `fx`, defaults `{ delay: 100, early: 60, word: 'x' }`, sliders `delay`, `early`, `word` and `absent`, each `MS_SCALE` → `['fx.early', 'fx.word', 'fx.absent']`. Defect: compare with `last` in place of `last / 2`. `it('throws at build for a slider off its middle, naming it')`: `buildRegistry` throws matching `` /`fx\.early`/ ``, and builds with only `delay` declared. Defect: the check's call removed from `buildRegistry`.
  - `browseModule.test.ts`: the "generates a control for every preference" case passes `browseModule.sliders` and expects `['coverSize', 'slider']`; `it('declares the cover size a slider on its own scale, 144 at the middle')`: the `coverSize` field's `initial` 144, `scale.valueAt(scale.last / 2)` 144, `scale.unit` `px`. Defect: `sliders` removed from `browseModule` (the field is `'number'`).
- [ ] **Step 2: Control tests** (jsdom):
  - `settingsFields.test.tsx`, field `{ key: 'delay', kind: 'slider', scale: MS_SCALE, initial: 100 }`: `it('draws a slider as a range over positions, its value and unit on the name’s row')`: at 120, the range's `min` 0, `max` 10, `value` 7, `aria-valuetext` `120 milliseconds`; the output `120 ms`; the label `delay` with `htmlFor` the range's id; label and output inside `.field-slider-head`, the range inside `.field-slider-track`. Defect: the range's `value` the raw value. `it('reports a position as its value, and its default on a double-click')`: an input at 2 → `onChange(70)`; a `dblclick` → `onChange(100)`. Defects: report the position; remove `onDoubleClick`. `it('draws a value off the scale at the nearest position, and one that is no number at its default')`: 123 → `120 ms` at 7; `'x'` → `100 ms` at 5. Defect: the output writes the raw value. The existing "takes its row styling from its caller" also renders the slider: `rowStyle` lands on `.field-slider-head` and not on `.field-slider`. Defect: `rowStyle` on the outer `div` (name, value and track then sit side by side).
  - `ViewMenu.test.tsx`: C1 §4's count case (~60, 5 labels, 2 selects and 1 range as Task 1 left it) and the doMock case (~104) stay green and unedited. `it('draws the cover size from its declaration: px beside the name, the position on the slider')`: output `144 px`, `max` 96, `value` 48, `aria-valuetext` `144 pixels`, label `Cover size`. Defect: `ViewMenu.tsx`'s `fieldsOf` without `browseModule.sliders` (a number box). `it('reports a slider position as its size')`: an input at 8 → `change('coverSize', 104)`, at 72 → `change('coverSize', 192)`. Defect: `valueAt` and `positionOf` swapped in `coverSizeScale`. `it('draws a second declared slider as it draws the cover size, with no edit to ViewMenu.tsx')`: ~104's doMock pattern, `defaults` gaining `delay: 100` and `sliders` gaining `delay: MS_SCALE`, keys `[...EVERY, 'delay']`: two ranges; the new one labelled `Delay`, output `100 ms`, `max` 10, `value` 5, `aria-valuetext` `100 milliseconds`, in the same three classes as the cover's; after an input at 9, a `dblclick` reports `change('delay', 100)`. Defect: `FieldControl` draws a slider only for `field.key === 'coverSize'`. `it('names no preference in the code that draws the panels')`: the raw text of `src/ViewMenu.tsx`, `src/Settings.tsx` and `src/settingsFields.tsx` (`?raw`, as `classNames.test.ts` reads sources) holds none of `coverSize`, `CoverSize`, `COVER_`. Defect: the prototype's `field.key === 'coverSize'` branch restored in `ViewMenu.tsx`, drawing through `FieldControl` (it draws the same, so only this case sees it).
  - `Settings.test.tsx`: `it('draws a declared slider as the View panel does, its name’s row in the panel’s row style')`: `modules` `[{ id: 'fx', label: 'Fixture', defaults: { delay: 100 }, sliders: { delay: MS_SCALE } }]`, `prefs` `{}`: a range at 5, output `100 ms`, `.field-slider-head`'s inline `display` `flex`; an input at 7 → `onChange('fx', 'delay', 120)`, a `dblclick` → `onChange('fx', 'delay', 100)`. Defect: `Settings.tsx`'s `fieldsOf` without `mod.sliders`.
- [ ] **Step 3: Implement** to the Interfaces above. Headers: `ViewMenu.tsx`'s "Generated, not written" stays true as written, with no exception to name; `settingsFields.tsx`'s says a slider is a kind its module declares, as a choice is; `browseModule.ts`'s "Nothing here is a settings screen" adds a number with a declared scale to a slider. `CoverSizeControl` and the scale's imports leave `ViewMenu.tsx`.
- [ ] **Step 4: Check** node+jsdom whole (0 failed; `classNames.test.ts` holds the renamed classes), `tsc -b`, biome; commit "The slider is a kind of setting: a module declares a number preference's scale and the generated panels draw it, the cover size first, with no key named in either panel".

### Task 2b: The slider in a browser (items 6–8, the save, the coarse mark)

**Files:**
- Modify: `web/src/styles/shell.css` (`.field-slider-track::after`, the coarse block)
- Test: create `web/tests/ViewMenu.browser.test.tsx` and `web/tests/settingsFields.browser.test.tsx`

**Interfaces:**
- Consumes: Task 2a's `.field-slider*` markup, `Settings` with a declared slider, `MS_SCALE`. Produces: nothing new.

- [ ] **Step 1: Browser tests** (`ViewMenu.browser.test.tsx`; Shell + Library mounted at 1280 as `Shell.browser.test.tsx`'s `at()` does, `library` mocked as there, the size seeded in real `localStorage` under `taiyaku.prefs.browse` and removed on finish):
  - `it('moves one position per arrow key')`: focus the range, real `ArrowRight` → output `146 px` and storage `coverSize` 146; two real `ArrowLeft` → `143 px`. Run alone with `-t` too. Defect: `step={2}`.
  - `it.each(['the thumb', 'the track'])('resets to 144 on a double-click on %s')`: from 240 px, `userEvent.dblClick(range, { position })` 4 px in from the range's right edge (the thumb at the end); from 96 px, at three quarters of its width (the track). Output `144 px`, storage 144, a grid tile 144 px wide. Defect: remove `onDoubleClick`.
  - `it('sets the name and the size on one row and the slider the panel’s width under them')` (item 5): the label's and the output's vertical centres within 1 px, the output right of the label; the range's width equals the panel's content width. Defect: `.field-slider-head` without `display: flex`.
  - `it('marks the middle under the thumb at 144')`: the `::after` computes `1px` by `6px` in `--line-strong`'s colour; its centre x (`range.left` + its computed `left` + 0.5) equals the range's centre ±0.5; from 96 px, a real press at the mark's x on the range's vertical centre leaves the range at position 48. Defect: `left: calc(40% - 0.5px)`.
  - `it('keeps the mark as near the track under a coarse pointer as under a fine one')`: the mark's top less the range's vertical centre, fine and with `coarseRulesOn()`, within 1 px of each other. **Red at the prototype** (the coarse range is 44 px and the mark sits under its box); fix in `shell.css` so the mark is placed from the range's centre, not its box's bottom, and stays `pointer-events: none`. Defect: the prototype's `bottom: 0`.
  - `it('saves and draws each step of a drag, before the release')`: a real press on the thumb at 144, `mouse.along` 30 px right, no release: storage's `coverSize` equals the output's size and is not 144, and a grid tile is that wide; then release. **The decision (gap 3): the slider saves on each step.** The shell holds one state, persisted on every change, so what is drawn is what is stored; a release-only save needs a second, draft state for the drag, and loses the drag if the panel closes on Escape or a press outside mid-drag. A `localStorage` write per step is small beside the render the drag already causes to make the covers follow. Defect: hold the drag in local state and call `onChange` from `onPointerUp`.
  - `settingsFields.browser.test.tsx`: `it('marks the middle of a slider in the reader’s Settings panel')`: `Settings` mounted at 1280 with Task 2a's `fx` module: the track's `::after` computes `1px` by `6px`, its centre x the range's centre ±0.5. Defect: the mark's rule written `.view-panel .field-slider-track::after` (the View cases stay green).
- [ ] **Step 2: Check** node+jsdom and browser whole; commit "The cover-size slider held by tests: one position per arrow key, a double-click resets, the mark under the thumb at any pointer and in either panel, and each step of a drag saved".

### Task 3: The View menu's rows (items 11, 12, the Firefox press)

**Files:**
- Modify: `web/src/ViewMenu.tsx` (the panel's click handler), `web/src/styles/shell.css` (only if a test calls for it)
- Test: `web/tests/ViewMenu.browser.test.tsx`

**Interfaces:**
- Produces: in `ViewMenu.tsx`, `function sendRowClick(event: React.MouseEvent<HTMLDivElement>): void`, the `.view-panel`'s `onClick`.

- [ ] **Step 1: Probe Firefox first** (no code yet). A Playwright Firefox script against the build on 127.0.0.1:4398 (login `local`/`pw`) logs, for presses on the "Continue strip" words moved 1, 2, 3 and 4 px, which of `click` on the label, `click` on the input and `change` fire, and whether the box toggled. The spec says Firefox drops the label's activation from 3 px; the handler below assumes the label still receives `click`. If it does not, stop and report the sequence rather than building on the assumption.
- [ ] **Step 2: The handler.** A click whose target is inside a `label` holding a checkbox, and is not that checkbox, is taken over: the label's own activation is cancelled and the box is clicked once, so one path toggles in both browsers and Chromium cannot toggle twice.

```ts
function sendRowClick(event: React.MouseEvent<HTMLDivElement>) {
  const target = event.target as Element
  const box = target
    .closest('label')
    ?.querySelector<HTMLInputElement>(':scope > input[type="checkbox"]')
  if (!box || target === box) return
  event.preventDefault()
  box.click()
}
```

  If biome flags the `div`'s `onClick`, a `biome-ignore` names why (the rows are labels and checkboxes with their own keyboard; this re-sends a label's click).
- [ ] **Step 3: Tests** (`ViewMenu.browser.test.tsx`, the library page's View open):
  - `it.each([1, 4])('toggles the Continue strip once for a press on its words moved %i px')`: real press on the words, move, release: the box flipped exactly once and the strip drawn or gone accordingly. Defect: drop `event.preventDefault()` (Chromium toggles twice, red at 4 px and 1 px).
  - `it('takes a click on a row’s words over from the label')`: a listener on `window` (bubble) records the `click` whose target is the words: `defaultPrevented` true, and the box saw exactly one `click`. Defect: remove the handler (Chromium still toggles; only this case and the Firefox drive see it).
  - `it.each(['left', 'right'])('toggles from the panel’s %s edge')`: a real click 2 px inside the panel's border at the row's middle toggles it; the row's computed `cursor` is `pointer`. Defect: remove `margin-inline` from `.view-panel label:has(> input[type="checkbox"])`.
  - `it.each(['the Cover size row', 'the panel’s padding'])('selects no words on a press that moves from %s')`: real press, `mouse.along` 60 px across the panel, release: `getSelection().toString()` is `''`. Defect: remove `user-select: none` from `.view-panel`.
- [ ] **Step 4: Check** browser whole; commit "A checkbox row in View toggles once from any press on it, Firefox's moved press included, and a press that moves selects none of the menu's words".

### Task 4: Exact covers from 900 px up, filled columns below (items 5, 9; gap 4's registration)

**Files:**
- Modify: `web/src/styles/library.css` (the `@property` block and `.library` tokens, ~113–121: trim to the cap; a `@media (width < 900px)` block for the fill, beside the existing ones ~616–640)
- Test: create `web/tests/coverLayout.browser.test.tsx`; update the expectations Task 1 assigned here in `Series.tile.browser.test.tsx` and `Series.panel.browser.test.tsx` (rows per width)

**Interfaces:**
- Consumes: `seriesPage({ coverSize })` (Task 1).
- Produces: below 900 px, `.grid` and `.volumes` lay `repeat(auto-fill, minmax(var(--tile-min), 1fr))` at `var(--tile-gap)`; `.library` sets `--tile-fill` to `0`, and to `1` in that same media block, which is the token `useContinueRow` (Task 6) reads to choose its layout, so the breakpoint is written once.

- [ ] **Step 1: Tests** (home grid through Shell + Library with `library.grid` mocked to `2n + 1` series for the width's `n`; series page through `seriesPage`). With `W` the list's content width and `g` its computed least gap, `n = max(1, ⌊(W + g)/(size + g)⌋)` in both layouts. Fixture checks below are computed from the CSS with no scrollbar (W = 1008 at 1280 beside the sidebar, 628 at 900, 867 at 899, 358 at 390), not measured; the tests compute from measured widths.
  - From 900 px up, `it.each` over `[width, size]` = `[1280, 96]`, `[1280, 144]`, `[1280, 240]`, `[900, 144]` on the home grid and on a series page, and `[1024, 151]` on the home grid only (seeded through storage, so `Shell` draws it at 152; `seriesPage` sets `--cover-n` as given): every cover box is the drawn size wide (±0.1); the first row holds `n` tiles; adjacent tiles in a row are equally far apart and never nearer than `g`; the row's last tile ends at the list's right edge; a short last row's tiles start at the first row's columns' lefts. (`n` is at least 2 here: at 900 px the area is 628 px, over two 240 px covers and a gap.) Fixture checks: 1280/144 is 6 to a row 28.8 px apart; 900/144 is 4, 17.3 px apart. Defects: `justify-content: space-between` removed (gaps not spread); `minmax(var(--tile-min), 1fr)` in place of the exact track at every width.
  - Below 900 px, `it.each` over `[390, 104]`, `[390, 144]`, `[390, 240]`, `[899, 144]` on both pages: the first row holds `n` tiles, each `(W − (n − 1)·g)/n` wide (±0.1) and never narrower than the size; adjacent tiles exactly `g` apart (±0.1); the row's last tile ends at the list's right edge (with `n = 1`, the one tile is the list's width); a short last row on the first row's columns. Fixture checks: 390/144 is 2 of 171 px, 16 apart (what main's `minmax(9rem, 1fr)` grid draws there); 390/104 is 3 of 112.2, 10.7 apart; 390/240 is 1 of 358; 899/144 is 5 of 160.6. Defects: the fill block removed (390/144 draws two 144 px covers 70 px apart); `(width < 900px)` → `(width < 800px)` (899 draws exact covers).
  - `it('derives the least gap from the size, and which layout is in force')`: `getComputedStyle(.library).getPropertyValue('--tile-gap')` is `9.6px` at 96, `12.8px` at 120, `16px` at 144 and at 240, `--tile-min` is `<size>px`, and `--tile-fill` is `0` at 900 and `1` at 899 (trimmed). Defects: delete `@property --tile-gap` (the value is no longer px); `0.6rem` → `0.5rem`; `--tile-fill: 1` set outside the media block.
  - No crossing case here: the grid is CSS alone, so nothing of the other layout can be left behind; the scripts that could leave one are Tasks 5 and 6, which drive the crossing.
- [ ] **Step 2:** Update the assigned expectations (rows per width, the gap between tiles) by the formulas above, the fill's below 900 px; keep each test's intent.
- [ ] **Step 3:** Trim the `.library`/`@property` comment to the why (≤ 8 lines); the curve's reasons that do not fit go to Task 10's notes. The fill block's comment says why the phone fills (the owner's ruling: a phone's spare room is wide gaps otherwise), within the cap.
- [ ] **Step 4: Check** browser whole; `node $W/scripts/check-comments.mjs` no longer names this block; commit "Covers are exactly the cover size from 900 px up, the spare room in the gaps, and fill the row below it, on the library grid and a series page, held by tests".

### Task 5: The volume panel over spread columns

**Files:**
- Modify: `web/src/tilePanel.ts` (`laidGap` over an exported pure `laidGapOf`), `web/src/Series.tsx` (~518: the observer and its comment)
- Test: `web/tests/tilePanel.test.tsx` (jsdom, exists), `web/tests/Series.panel.browser.test.tsx`

**Interfaces:**
- Produces: `laidGapOf(trackList: string, width: number): number | undefined` — `(width − Σ tracks)/(n − 1)` over the numeric tracks of a resolved `grid-template-columns`, `undefined` with fewer than two.

- [ ] **Step 1: unit tests** in `tilePanel.test.tsx`: `laidGapOf('144px 144px 144px', 500)` 34; `('[a] 144px [b] 144px', 300)` 12; `('144px', 300)` and `('', 300)` `undefined`. Defect: `n - 1` → `n`. With fewer than two tracks `laidGap` falls back to the computed `column-gap`.
- [ ] **Step 2: Browser tests** (Series.panel; the tile's width and the gap are read from the layout, tiles[0].width and tiles[1].left − tiles[0].right, never from the size):
  - `it.each([[1280, 144], [1280, 104], [899, 144], [390, 104]])('is two tiles and the laid gap wide at %i px (%i px covers)')`: the open panel's width is `2 × tile + that gap` (±1) from the tile's left. From 900 px up the tile is the size and the gap spread; below, the tile is the filled column and the gap the least (fixture checks: 899/144 is 2 × 160.6 + 16 = 337.2; 390/104 is 2 × 112.2 + 10.7 = 235.1). Replaces the `comfortable`/`compact` case. Defects: `laidGap` returns the computed `column-gap` always (red from 900 up); the panel laid from `--tile-min` in place of the tile's box width (red below 900 only, where a tile is wider than the size).
  - `it('is the area wide with one column (390 px, 240 px covers)')`: one filled column, the tile the list's content width (358), and the panel that width from the list's left. One column happens only below 900 px (from 900 up two always fit), where the tile is the area, so the gap cannot decide the width; what can break it is the gap itself. Defect: `laidGapOf` without its fewer-than-two rule (one track gives 0/0, a `NaN` width the style refuses, and the panel keeps its stylesheet's `200% + gap`, 732 px).
  - The two "is laid again …" cases (~389, ~416) assumed a tile that changes width with the area. Rewrite their fixture checks for fixed tiles. ~389 resizes 1280 → 800, which now crosses 900 px and fills the tile, so the tile observer alone would fire and the defect below would stay green: keep both widths at 900 or more (1280 → 1100: 6 then 5 columns at 144 px), and pick the tile the fixture checks name again (at 144 px `tiles[6]` opens row 2 at 1280, where at main's 120 px it ended row 1). After the window goes to 1100 (or the sidebar column widens), the tile's width is unchanged, the laid gap and the tile's column are not, and the panel follows (`expect.poll` on its left and width against the new laid gap). **Red at the prototype**: the observer watches the tile alone. Fix: the layout effect also observes the grid (`tile.parentElement`), whose width follows the area; correct the comment above it (from 900 up the tile is the cover size, not `1fr`). Defect: observe the tile only.
  - `it('is laid again across 900 px')`: open at 900/144, then `page.viewport` to 899 and back to 900; after each, `expect.poll` until the panel is `2 × tile + laid gap` wide (±1) against the tile's new width, 160.6 then 144, and within the area. Defect: the panel laid only on open (no observer).
- [ ] **Step 3: Check** node and browser whole; commit "A volume's panel spans the gap its columns are laid apart by and the tile's own width, and is laid again when the page area's width changes or the window crosses 900 px".

### Task 6: The Continue row's columns (item 2, both layouts; gap 4's fallback)

**Files:**
- Create: `web/src/continueRow.ts` (`useContinueRow` moved out of `Library.tsx`, with three pure helpers)
- Modify: `web/src/Library.tsx` (one `stripDrawn` condition for the hook and the JSX), `web/src/styles/library.css` (the `.continue` comment ~45–56: trim; `.continue ul`'s `--track` becomes `var(--continue-col, var(--tile-min))`)
- Test: create `web/tests/continueRow.test.ts` (node), `web/tests/Library.continue.browser.test.tsx`, `web/tests/Library.continue.scrollbars.browser.test.tsx`

**Interfaces:**
- Consumes: Task 4's `--tile-fill` (`1` below 900 px, else `0`).
- Produces: `useContinueRow(list: RefObject<HTMLUListElement | null>, drawn: boolean, coverSize: number, count: number): void`; `pxOf(value: string): number` (a `px` length as a number, `NaN` otherwise); `spreadGap(width: number, track: number, gap: number): number` (`n = max(1, ⌊(width + gap)/(track + gap)⌋)`; `(width − n·track)/(n − 1)` for `n > 1`, else `gap`); `filledColumn(width: number, track: number, gap: number): number` (the same `n`; `(width − (n − 1)·gap)/n`).
- The hook, on each measure, reads `--tile-fill`: at `1` it sets `--continue-col` to `filledColumn(…)` px and **removes** `--continue-gap` (the row keeps the least gap, as the filled grid does); otherwise it sets `--continue-gap` to `spreadGap(…)` px and **removes** `--continue-col`. A track or gap that is not a px length removes both. The `ResizeObserver` on the list is what runs it across 900 px: the sidebar column comes and goes there, so the list's width always changes.

- [ ] **Step 1: node tests:** `pxOf('16px')` 16, `pxOf(' 9.6px ')` 9.6, `pxOf('1rem')`, `pxOf('clamp(0.6rem, 1px, 1rem)')`, `pxOf('')` `NaN`; `spreadGap(1008, 144, 16)` 28.8, `spreadGap(358, 144, 16)` 70, `spreadGap(358, 240, 16)` 16, `spreadGap(144, 144, 16)` 16; `filledColumn(358, 144, 16)` 171, `filledColumn(358, 240, 16)` 358, `filledColumn(358, 104, 32 / 3)` ≈ 112.22 (`toBeCloseTo(…, 2)`), `filledColumn(867, 144, 16)` 160.6. Defects: `Math.floor` → `Math.round`; `n > 1` → `n > 0`; `(n − 1)·gap` → `n·gap` in `filledColumn`.
- [ ] **Step 2: Move and fix.** The hook lives in `continueRow.ts`, its doc ≤ 8 lines (the rest to Task 10's notes). **A non-px gap now falls back like a non-px track** (the prototype's `px('--tile-gap') || 0` counted columns at a zero gap, more than the grid has): if either is not a px length, remove both properties. The fill branch is new (the owner's 2026-11-15 ruling came after the prototype; its first round set a `--continue-col` this way, `git -C $W show ae48ce91 -- web/src/Library.tsx`). `Library.tsx` computes `stripDrawn` once and passes it to the hook and to the JSX's condition, so they cannot drift.
- [ ] **Step 3: Browser tests** (`Library.continue.browser.test.tsx`; Shell + Library as `Shell.browser.test.tsx`'s `at()`, `library.home` answering `continuing` and `library.grid` enough series for two rows):
  - `it.each` over `[width, size, entries]` = `[1280, 144, 1]`, `[1280, 144, 12]`, `[1280, 96, 20]`, `[1280, 240, 6]`, `[1024, 151, 12]`, `[899, 144, 12]`, `[390, 104, 6]`, `[390, 144, 6]`, `[390, 240, 3]`: every card is the grid's first tile's width (±0.5; from 900 up that is the drawn size, 152 for 151; below, the filled column, 160.6 at 899/144 and 171 at 390/144), and the first `min(n, entries)` cards' lefts are the grid's first columns' lefts (±0.5), also with the row overflowing and scrolled to its start. Defects: the row's `gap` back to `var(--tile-gap)` alone (no hook value; red from 900 up); the fill branch removed, so the hook sets the spread gap at every width (red below 900: cards 144 wide where the grid's are 171).
  - `it('switches layout across 900 px, leaving nothing of the other set, rendering nothing')`: 12 entries at 144; `page.viewport` 1280 → 899 → 1280 → 899, and after each `expect.poll` the cards are aligned and as wide as the grid's tiles; at 1280 the list's inline style holds `--continue-gap` and no `--continue-col`, at 899 the reverse; a `<Profiler>` around `Library` counted no commit across the four. Defects: the fill branch keeps `--continue-gap` (red at 899 after 1280: the row's gap the spread one); the exact branch keeps `--continue-col` (red at 1280 after 899: cards 160.6 wide).
  - `it('follows a live resize, rendering nothing')`: 1280 → `page.viewport(1100, 844)`; after `expect.poll`, the cards again over the grid's columns, and a `<Profiler>` around `Library` counted no commit during the resize (neither `Shell` nor `Library` listens for one today). Defect: no `ResizeObserver` in the hook.
  - `it('follows a slider change')`: real `ArrowRight` ×10 on View's range; cards and columns at the new size, aligned. A size change reaches the hook twice (its `coverSize` trigger, and the observer, since the cards' height changes), so dropping either alone stays green; the defect is a stale read: the track width read once per mount into a ref.
  - `it('holds at a 20 px root')`: `rootFontAt('20px')`, 1280/144/12: aligned. Defect: compute the gap in `spreadGap`'s caller from `coverSize` and 16 px per rem rather than the computed style.
  - `it.each([1280, 390])('has the gap or column in place when the row first appears at %i px')`: `IS_REACT_ACT_ENVIRONMENT = false` for the case (restored on finish), `library.home` a `deferred` resolved outside `act`; a `MutationObserver` on the host records, in the callback where `.continue ul` first appears, its inline `--continue-gap` (1280) or `--continue-col` (390), which must equal the grid's spread gap or first tile's width read afterwards (±0.5). Defect: `useLayoutEffect` → `useEffect`. If that defect does not turn it red, the case proves nothing: report it, do not keep it.
  - `it.each([1280, 390])('falls back to the least gap when the gap is not a px length at %i px')`: before mounting, delete the `@property --tile-gap` rule from its sheet (re-insert its text at its index on finish); the row has neither `--continue-gap` nor `--continue-col` and its computed `column-gap` is the least gap (16px at 144), its cards `--tile-min` wide. Defect: the prototype's `|| 0`.
- [ ] **Step 4: Scrollbars shown** (`Library.continue.scrollbars.browser.test.tsx`, the second Chromium launch): `it.each([1280, 899])('lines the cards up over the grid with a scrollbar drawn at %i px')` at 144 with 12 entries and enough series that `main` (1280) or the document (899) scrolls: aligned and as wide as the grid's tiles (±0.5). Defect: subtract a fixed 15 px in the hook's width.
- [ ] **Step 5: Check** node, jsdom and browser whole; commit "The Continue row's cards sit over the grid's columns at every size and width, exact from 900 px up and filled below, from the first paint and across the breakpoint, and fall back to the least gap where the gap cannot be read".

### Task 7: The Continue row's fade (item 3)

**Files:**
- Modify: `web/src/styles/library.css` (the fade's comment ~158–166: trim)
- Test: `web/tests/Library.continue.browser.test.tsx`

- [ ] **Step 1: Tests** (1280, 144, 20 entries, so the row scrolls):
  - `it('fades the sides it can still scroll to, rendering nothing')`: at scroll 0, `data-more-after` only; at half its travel, both; at the end, `data-more-before` only (each after a frame); the computed `mask-image` is not `none` in all three and holds `transparent 12px`, `50px` and `80px`; a `<Profiler>` around `Library` counted no commit across ten scroll steps. With 1 entry: neither attribute, `mask-image` `none`. Defects: invert `scrollLeft > 1`; `--fade: 60px`; the sides held in React state.
  - `it('stops fading when a smaller size lets every card fit')`: 7 entries at 1280/144 (6 columns, so `data-more-after`); real `ArrowLeft` ×48 on View's range (96 px, 9 columns): neither attribute, `mask-image` `none`. Defect: `resized` no longer calls `mark()` (the first run still does).
  - `it('lets a press through the fade')`: a real click 6 px in from the row's right edge on the card there opens `/read/<its slug>` (a route marker). Defect: an absolutely placed `::after` over the row's last 80 px.
  - `it('lifts the fade while a card has a focus ring')`: real Tab into the row: `mask-image` `none`. Defect: delete the `:has(:focus-visible)` rule.
- [ ] **Step 2: Check** browser whole; commit "The Continue row fades where more cards lie, lets a press through and lifts the fade for a focus ring, held by tests".

### Task 8: The Continue heading and the line under the row (items 1, 4)

**Files:**
- Modify: `web/src/styles/library.css` (the alignment comment ~90–101: trim)
- Test: `web/tests/Library.continue.browser.test.tsx`

- [ ] **Step 1: Tests:**
  - `it.each([['fine', '16px'], ['coarse', '16px'], ['fine', '20px']])('draws the Continue rule level with the Libraries rule at 1280 px (%s pointer, %s root)')`: the vertical centres of `.continue h2` and of the sidebar's Libraries `.sidebar-heading` differ by < 0.5 px (each rule is centred in its heading). Coarse through `coarseRulesOn()`; root through `rootFontAt`. Defects: drop `--seal` from `--side-rule`; delete the coarse block's `.continue h2` overrides.
  - `it('leaves the heading as it was below 900 px')`: at 390, `.continue h2`'s computed `margin-top` is `0px`. Defect: the `@media (width >= 900px)` wrapper removed.
  - `it('moves nothing above the Continue heading')`: `.page-head`'s box and View's top are equal with 0 and 12 entries. Defect: the alignment applied as `main`'s top padding where it holds a `.continue`.
  - `it('draws a line 16 px under the row and 16 px over the chips')`: the section's `::after` computes `height` `1px` and the `--line` colour; with `L` its top (`section.bottom − 1`): `L − ul.bottom` is 8 (the row's own 8 px padding makes 16 from the cards) and `chips.top − section.bottom` is 16. Defect: `.continue + .library-chips` margin removed.
  - `it.each(['the strip turned off', 'a search', 'nothing in progress'])('draws no line with %s')`: stored `continueStrip: false`; `/?q=…` with hits; `continuing: []`: no `[data-testid="continue"]`. With the strip off and with nothing in progress (a search draws no chips), the chips compute `margin-top` `24px` and `border-top-width` `0px`, and their top is the page head's bottom plus 24 (the page head has no margin). Defect: the line drawn as a `border-top` on `.library-chips`.
- [ ] **Step 2: Check** browser whole and `node $W/scripts/check-comments.mjs` (exit 0: the five blocks are now within the cap); commit "The Continue heading's rule lies level with the sidebar's, and a line under the row is drawn only while the row is, held by tests".

### Task 9: Cover images for the drawn size (Review Focus 4)

**Files:**
- Modify: `web/src/library.ts` (`COVER_LONG_EDGES`'s doc, `coverSources`, `coverSizes`), `web/src/Cover.tsx` (`sizes`)
- Test: `web/tests/library.test.ts` (~314, the O133 case), `web/tests/Library.test.tsx`'s cover case (~1282), `web/tests/Cover.browser.test.tsx`

**Interfaces:**
- Produces: `coverSources(url: string): { src: string; srcSet: string }` with width descriptors, each long edge's width at the box's 2:3 (`256 → 171w`, `512 → 341w`, `768 → 512w`); `coverSizes(size: number): string` in `library.ts`, `` `(width < 900px) ${2 * size + 32}px, ${size}px` ``; `Cover` sets `sizes` to `coverSizes(coverSizeOf(useBrowsePrefs()))`.
- Why the phone entry is a bound: below 900 px a cover is its filled column, `(W − (n − 1)·g)/n`, which `sizes` cannot write (it needs the floor in `n`). A filled column is always narrower than two covers and a gap (else a second would fit), and the least gap is at most 1rem, so `2 × size + 32` px never under-asks to a 32 px root. It may ask one step more than the drawn width needs on a phone (390/144 at 1×: 320 picks 512 where 171 needs 256); recorded in Task 10's notes.

- [ ] **Step 1: Tests:** node: the srcSet is `…w=256 171w, …w=512 341w, …w=768 512w`, `src` the 256; `coverSizes(144)` is `(width < 900px) 320px, 144px` and `coverSizes(240)` `(width < 900px) 512px, 240px`. jsdom: a cover drawn at a stored 240 has `sizes="(width < 900px) 512px, 240px"`. Browser (DPR 1): at 1280, 144 px picks `w=256` and 240 px `w=512`; at 390 and 240 px (one column, drawn 358 wide) `currentSrc` ends `w=768`. From 900 px up at 144 px every pick matches O133's at 1×, 2× and 3×; only a cover wider than about 170 px asks for more. Defects: keep the `x` descriptors (240 picks 256); drop `sizes` (the browser assumes `100vw`); `sizes` the size alone (390/240 picks `w=512`, 341w for a 358 px cover, the under-ask); `(widthh < 900px)`, a condition the browser drops (the same red, which shows the case would see the real condition fail to parse). A 3× screen still upscales a cover above 170 px (768 is the server's largest); recorded in Task 10's notes, not fixed.
- [ ] **Step 2: Check** all three projects; commit "A cover asks for the image its drawn size needs, a phone's filled cover included, so a large cover is not drawn from the smallest".

### Task 10: The docs

**Files:** `docs/status.md`, `docs/notes.md`, `docs/superpowers/specs/2026-08-14-library-browse-design.md` (§4), `docs/superpowers/specs/2026-10-18-sidebar-readability-about-design.md` (§2's table, ~95), `docs/superpowers/specs/2026-09-17-style-pass-design.md` (~573 "a 104 px cover", ~597 "three in compact"), `docs/superpowers/specs/2026-11-14-compact-series-tiles-design.md` (~30, ~190)

- [ ] **Step 1: Counts from the rebased file.** Run the sweep (`grep -c '^| O[0-9]'`, `grep -c '^| O[0-9]* | \[x\]'`, `grep -n -A1 '^What remains'`) on the Task 1 rebase (it holds #251's O289). If `origin/main` has moved since, rebase again first and recount.
- [ ] **Step 2: status.md, the five-part edit** as #250 did for O286 (`git log -1 -p 7c9a3773 -- docs/status.md`): O285's row `[x]` "**Closed <close date> by the cover-size slider: …**" and its site renamed to symbols that exist (`browseModule.ts`'s `coverSize` and `coverSizeOf`, `Shell.tsx`'s `--cover-n`, `library.css`'s `--tile-min`, `--tile-gap` and `--tile-fill`, `browseModule.ts`'s `coverSizeScale`, `settingsPanel.ts`'s `SliderScale`, `settingsFields.tsx`'s `FieldControl`, `module.ts`'s `sliderFaults`, `tilePanel.ts`'s `laidGapOf`; never the deleted `CoverSizeControl`); a new O288 row after O289's, opened and closed by this PR (the Continue row: heading level, covers the grid's in both layouts, fade, line; site `continueRow.ts`'s `useContinueRow` and `filledColumn`, `library.css`'s `.continue` rules; opened by the owner, 2026-11-14); O132's row gains the owner's 2026-11-14 ask to take it up as a quick follow-up; O285's open bullet gets its italic closing pointer; a dated paragraph "**The cover-size slider and the Continue row of <close date>**" (the spec's path) with a closed bullet each for O285 and O288; the head's count word (+1 row), "… are now closed — …, O285, …, O288 … leaving N", and "What remains: the N open items". The web test counts at "Six crates, …" (~183): node and jsdom read from Task 11's runs, the browser count in words; the Rust count from the gate.
- [ ] **Step 3: status.md's present-tense descriptions:** ~397's `density` → `coverSize`; the Later entry ~6994 ("Density on a series page … Density on the Library view") gets a dated italic pointer to O285.
- [ ] **Step 4: notes.md**, a section "## O285 and O288: the cover-size slider and the Continue row (<close date>)": the owner's rounds (pointer to the spec and the prototype commits); what the build found beyond the prototype (the comment guard red, the zero-gap fallback, the coarse mark, the panel not laid again on a resize, soft large covers, the Firefox label press and the handler); the two rulings after the prototype (2026-11-15: covers fill the row below 900 px, and the slider as a declared kind, with the declaration's shape and why its default must be its middle); the decisions (save per step and why; the `@property` fallback is reachable only where the registration is missing, tested by deleting the rule; the phone `sizes` is a bound that may ask one image step more than a filled cover needs, never less); the row's own scrollbar fades at its ends with the row, accepted; the reasons trimmed from the comments; the drive's results.
- [ ] **Step 5: Specs:** a dated italic pointer in each named place (density → the Cover size slider, O285; a 104 px Continue cover → the slider's size; "three in compact" → three at 104 px; §2's table "Density" → "Cover size"; C1 §4 → a number preference its module declares a slider (`ReaderModule.sliders`, O285) is drawn as one by `FieldControl`, so §4 holds whole: no control is drawn by name). History stays as written.
- [ ] **Step 6: Sweep.** grep the bare `O285`, `O288`, `density`, `compact` (the browse sense), `104 px`, `Density`, `CoverSizeControl` and `view-cover` across `docs/` and `web/src/`, and `fieldKind`/`FieldControl` in `docs/status.md`'s present-tense lines (a list of the inferred kinds gains the slider), and read every hit. Commit "Close O285 and open and close O288 in the status, the notes and the specs that described density".

### Task 11: Finish — the gate, the drive, the claim check

- [ ] **Step 1: Gate.** `env -u TAIYAKU_CORPUS -C $W NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/gate.log 2>&1; echo $? >| $E/gate.exit` (the repo's own script, never a copy; check `CARGO_TARGET_DIR` is unset or this worktree's). Exit 0 and the `all green` line quoted. A per-file coverage floor that fails moves the coverage (node/jsdom tests of the pure helpers), not the floor.
- [ ] **Step 2: Against main.** Re-run Task 1 Step 2's comparison: no test that passes on the base fails on the branch; every base test absent on the branch is accounted for by name. Then remove the base worktree (`git -C /home/davey/projects/taiyaku worktree remove --force /home/davey/projects/taiyaku/.claude/worktrees/continue-align-base`, in its own call).
- [ ] **Step 3: Build for the drive** into the lane's served directory: `env -C $W/web ./node_modules/.bin/vite build --outDir /tmp/claude-1000/-home-davey-projects-taiyaku/781af045-5210-4f1a-8048-2a8c5fafdad9/scratchpad/lane2/dist --emptyOutDir`. **Never restart or kill the server on 127.0.0.1:4398**; after the build, check it still answers.
- [ ] **Step 4: The drive**, Playwright with a real mouse and keyboard in Chromium and Firefox (model: `.superpowers/sdd/strip-scrollbar-evidence/drive.mjs`, `continue-row-evidence/seed.cjs`), at 1280×800, 1024×768, 899×800 (just under the breakpoint) and 390×844 (Chromium also with `hasTouch`/`isMobile` for a coarse pointer). Items 1–12 each PASS/FAIL/NOTE with a screenshot where it is visual; item 13 is code, held by Task 2a's tests, and no reader module declares a slider, so the drive sees it only as the View slider behaving as before. In Firefox in particular: presses on the Continue strip's words moved 1–4 px toggle exactly once; a double-click on the thumb and on the track; the mark under the thumb at 144; no words selected; the fade at start, middle and end; the cards over the grid's columns after a resize. Below 900 px, in both browsers: at 390 and 899, the home grid's and a series page's covers fill the row at the least gap, and the Continue cards are the grid's tiles' width over its columns, at 96, 144 and 240. **The crossing**, in both browsers: with the Continue row drawn and, on a series page, a volume's panel open, resize 1280 → 899 → 1280 (a window resize, not a reload): after each, the cards sit over the grid's columns at its tiles' width, the list's inline style holds only the property of the layout in force (`--continue-gap` from 900 up, `--continue-col` below), and the panel is two tiles and the laid gap wide. Nothing in progress: a second account made in Configuration → Users on the scratch server, signed in, shows no Continue section and no line. Record the browsers' versions and the user-agent read from the page. Report the phone at the default size (two 171 px columns 16 px apart, as main draws; computed, not yet seen) with a screenshot beside main's for the owner, and one at 240 px (one cover the row's width).
- [ ] **Step 5: Claim check.** Every figure in the notes, the rows and the PR body checked against a log or a re-measure; a correction re-checks its own commit and the PR body.

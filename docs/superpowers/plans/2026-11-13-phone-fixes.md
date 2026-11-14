# Phone fixes: O229, O171, O141, O172, and O274's 390 px half — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix five things a phone reader meets early. All five are web-only.
- O229: from inside a book, a tap reaches the account panel.
- O171: focusing the note editor's text box on a phone does not zoom the page.
- O141: at 390 px the readout line stays inside its row and ends in "…".
- O172: the note editor counts slots from 1, as the readout does.
- O274, its 390 px half: a long path in a stamp plan wraps.

**Architecture:**
- Task 1: Settings gets an "Account…" button. App closes Settings and opens the account panel in one commit, so focus goes back where closing Settings would have put it. Then Shift+A over Settings does the same.
- Task 2: the note editor's text box loses its inline font, and a new coarse-pointer rule gives it the 16 px floor.
- Task 3: the readout's truncation moves onto an inner span.
- Task 4: NotePanel prints its three slot numbers counting from 1.
- Task 5: the stamp plan's paths get `PathWithBreaks` and an `overflow-wrap` rule.
- Task 6: the closes and the narrowing in `docs/status.md`.
- Then the Finish.

There are no server changes.

**Tech Stack:** TypeScript/React 19 (`web/`), and the vitest projects `node`, `jsdom` and `browser`.

**Spec:** `docs/superpowers/specs/2026-11-13-phone-fixes-design.md`. It was approved on 2026-11-13 and amended the same day, after the pre-flight, in two places: the panel's module count (pre-flight M10) and O229's focus target (M11). The survey behind it is `.superpowers/sdd/phone-family-evidence/survey.md`, which gives file:line for every element and the tests that hold each one. The code was read at main 635b91bb; branch `phone-fixes` adds only the spec and this plan on top of it. Locations are given by symbol, and line numbers are hints taken at 635b91bb.

**Rulings** (binding; do not re-open):
- O229: the user picked the Settings entry on 2026-11-13, over a bar stop and a link to `/account`.
- The reader's top bar does not change. Its make-up was settled on 2026-09-19 (style-pass §8.3).
- The readout row stays one line and keeps `readoutRow`'s text, order and ellipsis (style-pass §8.4). Wrapping it is out.
- O274's 1280 px half stays as it is. The 2026-09-18 settlement (style-pass §7 ②) keeps the alignment panel and the metadata editor inside their tile, so O274 is narrowed, not closed.
- No two dialogs are open at once: Settings and the account panel each replace the other.
- Shift+A over Settings replaces Settings with the account panel, as Account… does (the owner, 2026-11-13; Spec gap 1).
- O229's focus target (spec as amended 2026-11-13): when the account panel closes, focus goes back where closing Settings would have put it. That is ⚙ when ⚙ opened Settings, and the page when a key pressed on the page did. This is the accessibility design's §3.3 (`2026-09-26-accessibility-pass-design.md:287-291`), a closed decision. It holds for Account… and for Shift+A over Settings.

**What the plan settles** (the builder's calls the spec left open):
- **O229: where the button goes.** "Account…" (with a single-character ellipsis) sits in Settings' head row, before Close, so Close stays at the row's end as it does in the account panel.
- **O229: the prop.** `onAccount` is a required prop on `Settings`. App is its only caller, and the type then holds App to passing it. The 18 mounts in `Settings.test.tsx` gain `onAccount={vi.fn()}`.
- **O229: how focus gets there.** It needs no focus code, and `focus.ts` does not change.
  - `usePanelFocus` records as a panel's opener whatever held focus when the panel opened. On closing, if focus is still in the panel or has dropped to `<body>`, it hands focus to that opener, or to the page if the opener has gone.
  - App's new `openAccount()` sets `settingsVisible` false and `accountOpen` true in one handler, so React commits both together. React runs every passive-effect clean-up of a commit before any of its new effects.
  - So Settings' clean-up hands focus to Settings' opener (or the page) first. That is exactly where closing Settings would have put it. The account panel's effect then records that element as its own opener, and hands focus back to it on closing.
  - The why-comment on `openAccount` says this, and says what breaks it: anything that moves focus in the handler, or that opens the account panel in a commit before Settings closes. Either way the panel records the wrong opener.
- **O171.** A separate `@media (pointer: coarse)` rule, `.docked-panel :where(input, select, textarea), .docked-panel .reader-field { font-size: max(16px, var(--text-ui)); }`, is placed directly after the dialog floor in `reader.css`. That puts it after `.reader-field`, so Biome's `noDescendingSpecificity` is satisfied, and its (0,2,0) prelude beats `.reader-field`'s own `font: inherit` by specificity.
  - It sits only inside a coarse block, so the O152 gate in `base.css.test.ts` collects nothing new.
  - The dialog floor's prelude is untouched, so `reader.css.test.ts`'s exact-prelude case still finds it.
- **O171: the tests.** The existing floor is tested in two halves: `reader.css.test.ts` pins the rule's text, and the browser project reads computed sizes with `coarseRulesOn()` from `tests/support/pointer.ts`. This plan does the same: a node case for the new rule, and an `App.browser.test.tsx` case under each pointer.
- **O141: which element truncates.** The row `div.reader-readout` keeps its height, flex centring and padding. Its only child becomes a `<span>` with an inline style, so it needs no class and no CSS rule (`classNames.test.ts`): `overflow: 'hidden'`, `textOverflow: 'ellipsis'`, `whiteSpace: 'nowrap'`. Those three truncation properties leave the row.
  - A flex item is blockified, so the span is a block container that `text-overflow` applies to.
  - `overflow: 'hidden'` also lets the span be narrower than its text. A flex item that is a scroll container has no automatic minimum width (Flexbox §4.5), so no `minWidth: 0` is needed.
  - Short text stays centred, because the span is only as wide as its text.
- **O274.** The CSS goes in `library.css`: `.archives .path { overflow-wrap: anywhere; }`.
  - *Amended after Task 5:* the wrap rule moved onto the panels' roots; see the spec's dated O274 amendment.
  - It names `.archives` and `.path`, both of which leave `UNREAD`.
  - It does not name `.archive`, which is a hook of `MetadataEditor.test.tsx`; naming it would trip the "exempts only classes still written and still in no selector" case.
  - `span.path` draws `<PathWithBreaks path={archive.path} />`.

**New interfaces:**
- `SettingsProps.onAccount: () => void`. It is called by Account…; the caller closes Settings and opens the account panel.
- In `App.tsx`, `openAccount(): void`, which closes Settings and opens the account panel in one handler. It reads no state, only the two setters, so the keydown listener's frozen closure may call it (Step 5).

**Spec gaps** (raised, not resolved here):
1. **Shift+A while Settings is open puts two modals up**, which the anti-goal forbids. This was found by reading the code, not run. Pre-existing on main.
   - `keyPlan`'s `open-account` stands down only for the note editor and an open account panel.
   - App's `open-account` case sets `accountOpen` alone.
   - Both panels are `.reader-dialog` at z-index 20, and `<Settings>` is rendered after `<Account>`. So Settings is drawn over the account panel, which now holds focus.
   - The spec says "Shift+A stays" and does not say what Shift+A does over Settings. **RULED by the owner 2026-11-13, "Fix it here": Shift+A over Settings replaces Settings with the account panel, as Account… does.** Task 1's Step 5 does it.
2. **The approved spec's focus rationale did not match the code. Resolved by the spec's amendment of 2026-11-13 (pre-flight M11).**
   - The spec as approved sent focus to ⚙ unconditionally, saying that "O161's fallback would otherwise put focus on the page". But React runs every effect clean-up of a commit before any new effect, so Settings' clean-up hands focus back to its own opener before the account panel records one.
   - Unconditional ⚙ would also have broken §3.3 when a key pressed on the page opened Settings.
   - The amended spec takes Settings' own target, which that ordering already delivers ("What the plan settles").
   - When a press on ⚙ does not focus it (as with jsdom's `click`), Settings' opener, and so the account panel's, is whatever held focus before, as it is for Settings itself today.
3. Not a spec gap, but missing from the survey: `App.test.tsx`'s "n opens an editor anchored to the slot on screen" also pins the 0-based print (`toContain('raw slot 2')`). Task 4 changes it.

## Global Constraints

- **Comments.** A code comment says why: the reason, the invariant, what breaks if it changes. It never carries history or measurements, and it may point at an O-number. `scripts/check-comments.mjs` enforces part of this through the gate, including each area's comment share in `scripts/comment-budget.json`. If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **Copy.** The button's text is exactly `Account…`, with U+2026 and no space. Tests assert it as a literal. Every user-facing string a test checks is written out in the test, never computed through the code under test.
- **Gate.** Run the repo's own `scripts/check-all.sh`, never a copy. It `cd`s to the git top level of the directory it is run from, so run it from the main checkout:
  - Command: `builtin cd /home/davey/projects/taiyaku && E=/home/davey/projects/taiyaku/.superpowers/sdd/phone-family-evidence && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`.
  - Wait for load < 24 (`cat /proc/loadavg`) before starting, and record it.
  - Read the `.exit` file and quote the `all green` line. Never read the harness's exit code, and never pipe the gate through `tail`.
- **Web, per task.** Run from `web/` by absolute path. Never a bare `cd` (it is zoxide here), never `pnpm exec`.
  - Tests: `builtin cd /home/davey/projects/taiyaku/web && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`. Read the summary lines from the log.
  - Then run `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests` the same way.
  - Run whole projects, never a named file. `-t` is only for watching one mutation fail.
  - A task that touches CSS or a `*.browser.test.tsx` file runs the whole browser project, which CI never runs.
- **Browser crashes.** A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing. Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project. The Filmstrip "Tab brings focus to the rail" case also goes red under load, on main too.
- **Pre-existing failures.** A failure in a file the task did not touch counts as pre-existing only if the same command fails at main 635b91bb, not at another commit on this branch. Check it in a detached scratch worktree under the session scratchpad (copy `web/node_modules`, never symlink it; copy `crates/taiyaku-wasm/pkg`), not by switching this checkout. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one:
  - Put the named defect back by hand and watch the test fail.
  - Restore it, and check with `git diff` that the restore is complete.
  - Record each in the task report: the defect, the failing test, and the assertion's message.
  - A fixture must tell right from wrong. Each task's test step says how its fixture does, and the test asserts that first.
- **No new class without a rule.** `classNames.test.ts` fails on a class no selector names. The O141 span carries an inline style and no class.
- **Commits.** One commit per task, its subject naming the O-number(s); Task 1 makes two, at Steps 4 and 5. Never `--amend`. Before each commit, check that the previous task's hash is an ancestor of `HEAD`.
- **Branch and order.** Branch `phone-fixes`, in the main checkout `W=/home/davey/projects/taiyaku`; no worktree. The tasks run one after another.
- **Evidence.** `E=/home/davey/projects/taiyaku/.superpowers/sdd/phone-family-evidence`, absolute, and set in every Bash call that uses it (shell state does not persist). Logs and each task's report (`$E/T<n>-report.md`) go there, written by Bash (`cat >| … <<'EOF'`), because the Write tool is refused under `.superpowers/`. Nothing goes under `target/`.
- **No Rust.** No Rust file changes. If a task finds it needs one, it stops and reports.

## Review Focus

1. **Settings opened by a key pressed on the page, with ⚙ on screen**: closing the account panel puts focus on the page, not ⚙ (§3.3). Task 1, Step 1, case (c), and Step 5, case (e).
2. **Shift+A with Settings closed and focus on a bar stop**: closing the panel hands focus back to that stop, as O161 has it, and not to ⚙. Task 1, Step 5, case (f).
3. **A readout that fits** is left as it was, centred and whole, at 390 and 1280. The long text is whole at 1280. Task 3, Step 1, cases (b) and (c).
4. **The note editor under a mouse** keeps 14 px, so desktop does not change. Task 2, Step 1, the fine-pointer case.
5. **A path with a single name longer than the tile**, with no `/` or `-` to break at, stays inside its tile. Task 5, Step 2.

---

### Task 1: An Account… entry in Settings (O229)

Spec "Design", O229. Survey §O229, option (b), and its focus note.

**Files:**
- `web/src/Settings.tsx`:
  - `SettingsProps`: add `onAccount`, documented as above.
  - The head row `.reader-panel-head` (~151-156): add `<button type="button" className="reader-button" onClick={onAccount}>Account…</button>` before Close.
  - The component's doc: one sentence saying the entry is not a module, so a new module still needs no edit here.
- `web/src/App.tsx`:
  - `openAccount()` beside `accountOpen`'s state (~751): `setSettingsVisible(false)` and `setAccountOpen(true)`, and nothing that moves focus. Its why-comment is the one in "What the plan settles".
  - `<Settings onAccount={openAccount}>` (~3061).
- Tests:
  - `web/tests/Settings.test.tsx`: the 18 mounts gain `onAccount={vi.fn()}`, plus one new case.
  - `web/tests/App.test.tsx`, describe "the panels and focus" (~4155): new cases. Use the helpers `open`, `press`, `click`, `button`, `dialog`, `pageArea`, `settle` and the describe's `pressHere`.

**Interfaces:**
- Produces `SettingsProps.onAccount` and App's `openAccount()`. Step 5 uses `openAccount()`; no later task uses either.

- [ ] **Step 1: Write the failing tests.**
  - (a) `Settings.test.tsx`, "draws Account… in its head row before Close, and hands a press to onAccount alone":
    - The `.reader-panel-head` buttons' `textContent`s equal `['Account…', 'Close']`.
    - Clicking Account… calls `onAccount` once and `onClose` not at all.
  - (b) `App.test.tsx`, `it.each` over Close and Escape: "Settings opened by ⚙, then Account…: the account panel hands focus back to ⚙ on %s".
    - Setup is the house pattern of "Settings opened by ⚙ hands focus back to ⚙": `press('Tab')`, then `act(() => gear.focus())`, then `click(gear)`.
    - Then `click(button(settingsDialog, 'Account…'))`.
    - Assert `dialog(host, 'Settings')` is null, `dialog(host, 'Account')` contains `document.activeElement`, and exactly one `[role="dialog"][aria-modal="true"]` is in `host`.
    - Close it by the panel's Close button, or by `pressHere('Escape')`.
    - Assert `document.activeElement` is ⚙.
  - (c) `App.test.tsx`, "Settings opened by s on the page, then Account…: Escape hands focus back to the page, not ⚙":
    - `press('Tab')` to bring the bars up, then `act(() => pageArea(host).focus())`, `pressHere('s')`, and click Account….
    - Assert that the account panel holds focus and Settings is gone.
    - The fixture tells: assert that ⚙ is drawn (`button(host, 'Settings')` is defined) just before the Escape, so the page and ⚙ are both candidates.
    - `pressHere('Escape')`; `document.activeElement` is `pageArea(host)`.
  - Run the jsdom project. (a), (b) and (c) all fail, and `tsc -b` fails on the 18 mounts until Step 2.
- [ ] **Step 2: Implement** as under Files. Run the jsdom project to green. The existing "opens the account panel on A and closes it on Escape" case stays green (Shift+A stays).
- [ ] **Step 3: Watched mutations.**
  - Wire Account… to `onClose`: (a) fails.
  - Drop `setSettingsVisible(false)` from `openAccount`: (b) fails on the dialog count.
  - Drop `setAccountOpen(true)`: (b) fails.
  - Always focus ⚙ in `openAccount` (for example, `document.querySelector<HTMLElement>('.reader-icon-button')?.focus()` first): (c) fails, with focus on ⚙.
  - Focus the page area in `openAccount` first: (b) fails, with focus on the page.
  - Open the account panel in a commit of its own before Settings closes (`flushSync(() => setAccountOpen(true))`, then `setSettingsVisible(false)`): (b) fails. The panel records Account… as its opener, and Account… has gone by the time it closes.
- [ ] **Step 4:** Run `tsc -b`, `biome check`, and the whole browser project: `App.browser.test.tsx`'s "Settings keeps Tab inside it" walk now crosses Account…. Commit `… (O229)` and write `$E/T1-report.md`.
- [ ] **Step 5 — Shift+A over Settings (Spec gap 1, ruled in by the owner 2026-11-13).**
  - App's `open-account` case (~871) calls `openAccount()`.
  - With Settings closed, `setSettingsVisible(false)` changes nothing, so Shift+A behaves as today and no Settings-open condition is needed. `openAccount` reads no state, so the keydown listener's frozen closure is safe to call it from.
  - Tests, in "the panels and focus":
    - (d) "Shift+A over Settings opened by ⚙ replaces it, and Escape hands focus back to ⚙". Use (b)'s setup, then `pressHere('A', true)`. Assert Settings is null, Account holds focus, and exactly one aria-modal dialog is up. Then `pressHere('Escape')`; focus is on ⚙.
    - (e) "Shift+A over Settings opened by s on the page hands focus back to the page". Use (c)'s setup and its ⚙-is-drawn assertion, then `pressHere('A', true)` and `pressHere('Escape')`; focus is on `pageArea(host)`.
    - (f) "Shift+A with the bars up and Settings closed hands focus back to the stop it was pressed on". `press('Tab')` puts focus on the Library stop; assert that first. Then `pressHere('A', true)`, then `pressHere('Escape')`; focus is on Library.
    - Run the jsdom project. (d) and (e) fail on Step 4's code; (f) passes there and stays as a guard.
  - Watched mutations:
    - Restore the old one-line case (`setAccountOpen(true)` alone): (d) fails on the dialog count.
    - Always focus ⚙ in `openAccount`: (e) and (f) fail.
  - Rerun Step 4's checks, commit `… (O229)` as a second commit, and append to `$E/T1-report.md`.

---

### Task 2: The note editor's text box takes the 16 px floor (O171)

Spec "Design", O171. Survey §O171, "Smallest fix" and "What the O152 floor gate checks".

**Files:**
- `web/src/NotePanel.tsx`: the textarea's inline style (~183). Drop `font: 'inherit'`; keep `width` and `resize`. `.reader-field` already sets `font: inherit` in CSS.
- `web/src/styles/reader.css`:
  - The new coarse block from "What the plan settles", directly after the dialog floor's block (~516-521).
  - Its comment says why it is a separate rule: the docked panel is not a dialog, and the dialog floor's selector is pinned exactly.
- Tests:
  - `web/tests/styles/reader.css.test.ts`: a sibling describe after "the dialog panel's controls clear the iOS zoom floor". Hoist that describe's `flat` helper to file scope if both need it.
  - `web/tests/App.browser.test.tsx`: a new describe. Import `coarseRulesOn` from `./support/pointer`.

- [ ] **Step 1: Write the failing tests.**
  - Node, "gives the docked panels' controls a coarse-pointer font-size of at least 16px (O171)": `flat(readerSource, '.docked-panel :where(input, select, textarea), .docked-panel .reader-field', 'font-size')` equals `['max(16px,var(--text-ui))']`.
  - Browser, `it.each([['coarse', '16px'], ['fine', '14px']])`, "O171: the note editor's text box under a %s pointer is %s":
    - At 390×844, call `onTestFinished(coarseRulesOn())` for the coarse case only.
    - `reader(390, 844)`, `press('n')`, then `until` the `[aria-label="Note"]` dialog is drawn.
    - `getComputedStyle(textarea).fontSize` is the expected value.
    - The fine case is what tells a floor lifted out of the coarse block from a correct one.
  - Run the node and browser projects. The node case and the coarse case fail.
- [ ] **Step 2: Implement.** Run node, jsdom and the whole browser project to green. `reader.css.test.ts`'s exact-prelude and ordering cases, and `base.css.test.ts`'s O152 gate, stay green unedited.
- [ ] **Step 3: Watched mutations.**
  - Put the inline `font: 'inherit'` back: the coarse case fails.
  - Delete the new rule: the node case and the coarse case fail.
  - Lift the rule out of `@media (pointer: coarse)`: the fine case fails.
  - Drop `.docked-panel .reader-field` from the prelude: the node case fails.
- [ ] **Step 4:** Run `tsc -b`, `biome check`, then commit `… (O171)`. Write `$E/T2-report.md`.

---

### Task 3: The readout ends in "…" inside its row (O141)

Spec "Design", O141. Survey §O141, "Longest text" and "Smallest fix".

**Files:**
- `web/src/Chrome.tsx`:
  - `readoutStyle` (~159-169): keep `height`, `display`, `alignItems`, `justifyContent`, `gap` and `padding`, and drop the three truncation properties.
  - A new `readoutTextStyle` holds them, and nothing else.
  - The row's JSX (~410) wraps `{row}` in `<span style={readoutTextStyle}>`.
  - Rewrite the comment above the two styles so it says why the truncation is on the span: `text-overflow` does nothing on a flex container.
- `web/src/modules/readouts.ts`: `readoutRow`'s doc (~426-431) has two sentences that name where the truncation lives. Check each against the span, and edit only one that names the wrong element:
  - "`Chrome.tsx` pairs this with a `white-space: nowrap` row of fixed height". The row keeps its fixed height, but `nowrap` moves to the span.
  - "`Chrome.tsx`'s `overflow: hidden` + `text-overflow: ellipsis`".
- Tests: `web/tests/Chrome.browser.test.tsx`, a new describe after the mark's, using the file's `inReader` and `props`. `Chrome.test.tsx`'s row cases (~171-214) stay as they are.

**The longest text** (survey; all five readouts on, with `readouts` set to all `true`):
- `position: 'digital-colored-comics slot 188 / 195 · p. 190 · 96%'`
- `health: 'fragmented pair · no counterpart here'`
- `time: '12m read · ~22m left'`
- `chapter: '~c002 · 3 of 7'`
- `notes: '12 notes here of 140'`

- [ ] **Step 1: Write the failing browser cases.** "O141: the readout row at a phone's width" holds three cases. `row` is `.reader-readout`, and `text` is its only element child. Each case first asserts that `row.children.length` is 1 and that `row.childNodes.length` is 1 (no bare text beside the span).
  - (a) At 390×844, the long text, "ends in … inside its padding, on one line, 32 px high":
    - The fixture tells: `text.scrollWidth > text.clientWidth` (the text is longer than its box). This is asserted first.
    - `text.textContent` equals the whole joined string, written out as a literal.
    - `getComputedStyle(text).textOverflow` is `'ellipsis'`.
    - `text`'s box lies within `row`'s left + 12 and right − 12, ±0.5.
    - `text`'s top and bottom lie within `row`'s.
    - `row`'s height is 32.
  - (b) `it.each` over 390×844 and 1280×800, the text `'slot 1 / 3'` (`position` alone on): "a readout that fits is whole and centred".
    - `text.scrollWidth <= text.clientWidth`.
    - The glyphs' centre is within 1 px of `row`'s centre. Measure the glyphs as `document.createRange()` over `text`'s contents, then `getBoundingClientRect()`. The span's box is not enough: a span stretched across the row is centred however its text sits.
  - (c) At 1280×800, the long text: "is whole at a desktop's width": `text.scrollWidth <= text.clientWidth`.
  - Run the whole browser project. (a), (b) and (c) all fail on the unchanged code, at the child check.
- [ ] **Step 2: Implement.** Run the whole browser project and the jsdom project to green. `App.browser.test.tsx`'s bar height case (`1 + strip + 2 + 32`) and `inkWeight.browser.test.tsx`'s readout look stay green unedited.
- [ ] **Step 3: Watched mutations.**
  - Drop `overflow: 'hidden'`: (a) fails. The span's minimum becomes its text's width, and it spills past both paddings; the failure shows at the fixture check, then the padding check.
  - Drop `textOverflow`: (a) fails.
  - Drop `whiteSpace: 'nowrap'`: (a) fails at its fixture check, because the wrapped text no longer overflows its box.
  - Give the span `maxWidth: 120`: (c) fails.
  - Give the span `flex: 1`: (b) fails at the centring check.
  - Restore the old markup (the text straight in the row): (a) fails.
- [ ] **Step 4:** Run `tsc -b`, `biome check`, then commit `… (O141)`. Write `$E/T3-report.md`.

---

### Task 4: The note editor counts slots from 1 (O172)

Spec "Design", O172. Survey §O172.

**Files:**
- `web/src/NotePanel.tsx`, three sites. Each prints its value + 1; the stored and navigated values stay zero-based (`onGo` still hands `entry.here` to `goToSlot`).
  - The editor's heading, `{draft.slot}` (~165).
  - The review row, `{entry.note.slot}` (~338).
  - The travelled-note suffix, `${entry.here}` (~343).
- Tests:
  - `web/tests/NotePanel.test.tsx`: every case that prints a slot (~110, ~118, ~129, ~142, ~570, ~591-592, ~640-642).
  - `web/tests/App.test.tsx`: "n opens an editor anchored to the slot on screen" (~1506).

- [ ] **Step 1: Change the tests first**, to whole strings and 1-based numbers.
  - Each assertion becomes `toBe`/`toEqual` on the whole `p` or `.reader-meta` text, never `toContain`.
  - Read the new strings off the slot each fixture stores, plus one. For example, the region case with `slot: 7` becomes `'raw slot 8 · region'`, and the travelled case with `slot: 5, here: 7` becomes `'raw slot 6 · en-mono slot 8'`.
  - The two `toContain` checks at ~570 and ~591 become whole-row strings.
  - ~610's count of one "slot" stays as it is.
  - App's case asserts the heading's whole text for stored slot 2, which the readout calls slot 3 of 6.
  - Run the jsdom project; every changed case fails.
- [ ] **Step 2: Implement.** Run the jsdom project to green.
- [ ] **Step 3: Watched mutations**, one site at a time:
  - Revert the heading: the four heading cases and App's case fail.
  - Revert the row: ~570, ~591 and ~640 fail.
  - Revert the suffix: ~591 and ~641 fail.
- [ ] **Step 4:** Run `tsc -b`, `biome check`, then commit `… (O172)`. Write `$E/T4-report.md`.

---

### Task 5: A stamp plan's paths wrap (O274, the 390 px half)

Spec "Design", O274. Survey §O274, "Smallest fix".

**Files:**
- `web/src/MetadataEditor.tsx`: `planBlock` (~596-610). `span.path` draws `<PathWithBreaks path={archive.path} />`, imported from `./PathWithBreaks`.
- `web/src/styles/library.css`: `.archives .path { overflow-wrap: anywhere; }`, beside `.stamp-waits` (~433), with a one-line why.
- `web/tests/classNames.test.ts`: remove `'archives'` and `'path'` from `UNREAD`.
- Tests:
  - `web/tests/MetadataEditor.test.tsx`, beside "names every archive with its own size, one row each", using its `PLAN`.
  - `web/tests/MetadataEditor.browser.test.tsx`: a new describe, following the O51 case's mocks of `stampPlan` and `deriveStatus` and its `plan-stamp` click.

- [ ] **Step 1: The jsdom case**, "draws each path with a break after each / and -, its text whole":
  - For each row, `.path`'s `textContent` equals the archive's path.
  - The number of `.path .path-break` elements equals the count of `/` and `-` in the path, which is 3 for `'/lib/Series - v01.cbz'`.
  - Run the jsdom project; it fails.
- [ ] **Step 2: The browser case**, "O274: a stamp plan's path stays inside its tile at 390 px":
  - Viewport 390×844. Mount `<div className="library"><ul className="volumes"><li>{editor}</li><li /></ul></div>`, where the editor is `MetadataEditor` with `canStamp`. Click "What would this change?" and wait for `.archives .path`.
  - The plan's one archive is `/mnt/user/manga/Kagami_no_Kuni_no_Harisugawa_Complete_Digital_Edition_Volume_01.cbz`. Its last name has no `/`, `-` or space.
  - "The tile" is the `.volumes > li`, not `.path`'s `closest('li')`, which is `li.archive`.
  - The fixture tells, and is asserted first: the last name, measured on one line in the path's own font, is wider than the tile's content box. Measure it with a `white-space: pre` probe span, re-written in the file as `Libraries.shell.browser.test.tsx`'s local `widestIn` does it. The new viewport case also needs `page` from `vitest/browser`, which the file does not import yet.
  - Per element: the `.path`'s right edge is ≤ the tile's right edge (+0.5).
  - Then `document.documentElement.scrollWidth` ≤ `window.innerWidth`.
  - If the tile overflows for a reason other than the path (another part of the editor wider than its 187 px), stop and report NEEDS_CONTEXT rather than changing the fixture or the editor.
  - Run the whole browser project; it fails.
- [ ] **Step 3: Implement the component change and the CSS.** Before editing `UNREAD`, run the node project and watch "exempts only classes still written and still in no selector" fail, naming `archives` and `path`. Then edit `UNREAD`, and run node, jsdom and the whole browser project to green.
- [ ] **Step 4: Watched mutations.**
  - Draw `{archive.path}` plain: the jsdom case fails.
  - Delete the CSS rule: the browser case fails.
- [ ] **Step 5:** Run `tsc -b`, `biome check`, then commit `… (O274)`. Write `$E/T5-report.md`.

---

### Task 6: The closes and the narrowing in `docs/status.md`

Read the Open items section: rows O141, O171, O172, O229 and O274. Find which have prose bullets with `grep -a -n '^- \*\*O<n>' docs/status.md`; at 635b91bb only O274 had one.

- [ ] **Step 1: Close O141, O171, O172 and O229** with the five-part edit:
  1. The row's checkbox.
  2. The row's text, rewritten to say what was done, by symbol, with no line numbers. Name the choices from "What the plan settles":
     - O229: the entry's place; focus going back where closing Settings would have put it; and Shift+A over Settings now replacing Settings, which put two modals up on main before this branch (Spec gap 1). No separate O-number for that.
     - O171: the separate coarse rule.
     - O141: the span.
     - O172: the three sites.
  3. A notes bullet for each. None exists for O141, O171, O172 or O229.
     - Add `- **O<n> — <title, past tense>. Closed.**` after the last section (O281's, ~4755).
     - Put them under an intro naming this PR, the four closes and O274's narrowing, shaped like the one at ~4745-4750.
     - O229's bullet also names Shift+A over Settings.
  4. The "… are now closed — O1, …, leaving …" sentence. Insert the four O-numbers in order, and move both counts by four.
  5. The "What remains: the … open items" line. Its count word may wrap across a line break, so grep the opener and read the next line. Check whether the sentences after it count any of the four.
- [ ] **Step 2: Narrow O274.**
  - The row stays `[ ]`. Its text says:
    - The 390 px half is fixed: `PathWithBreaks` in `planBlock` and `.archives .path`'s `overflow-wrap`.
    - What remains is the 1280 px column. The 2026-09-18 settlement (style-pass §7 ②) keeps that panel inside its tile, so moving it is the owner's decision, not a defect fix.
  - Its notes bullet is rewritten to say the same, still marked "Open."
  - The counts do not move for O274.
- [ ] **Step 3: Web test counts.** Measure, do not add. Run the node, jsdom and whole browser projects with `--reporter=default`.
  - Set the "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one" figures from the node and jsdom runs' summary lines.
  - Set the "one thousand and N more in a **browser** project" figure from the browser run's summary line.
  - The Rust figures do not move.
- [ ] **Step 4: Sweep.**
  - Run `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`, and check them against the sentences.
  - Grep each bare O-number (`O229`, `O171`, `O141`, `O172`, `O274`) across `docs/` and `web/src/`, and read every hit. Prose elsewhere that speaks of one as open is marked or corrected. Historical narrative is marked, not rewritten.
- [ ] **Step 5:** Commit `Close O229, O171, O141, O172 and narrow O274 in status.md`. Write `$E/T6-report.md`.

---

*Added after the PR opened, at the owner's request (2026-11-13).* Task 7, the editor's Title field leaving the tile title's face, ran from its own brief, `$E/T7-brief.md` (09697925). Tasks 8 and 9 build the spec's section "The Title box opens downward".

### Task 8: The Title box opens downward while it is edited (O274)

Spec, "The Title box opens downward". It builds on 09697925: at rest the box is the tile-wide one Task 5 and Task 7 left, and the 1280 px bound case in `MetadataEditor.browser.test.tsx` must stay green.

**Files:**
- `web/src/MetadataEditor.tsx` (~743-753): the Title field. One element keeps focus and caret from rest to open (no swap that moves focus); its accessible name stays "Title". A textarea styled to one line at rest is the expected shape; the builder may choose otherwise and says why in the report.
- `web/src/styles/library.css` (~448-458, the `.metadata-editor input` bound): rules for the field at rest and open. At rest it looks as it does at 09697925.
- Placement (which way it opens, how wide) is measured on focus against the tile and the page area (`.library > main`), since CSS cannot tell a tile's column. The page area must not gain a horizontal scroll from it.
- `web/tests/classNames.test.ts` for any new class.
- Tests: `web/tests/MetadataEditor.test.tsx` (jsdom, the `MetadataEditor` describe at ~120) and `web/tests/MetadataEditor.browser.test.tsx` (the O274 describe at ~210 and its `planInTile` helper at ~314, which already mounts a tile in `.library > main` at 390 and 1280 px).

**Interfaces:** none new outside the component; `submit` and the save call are unchanged.

- [ ] **Step 1: jsdom cases**, each watched failing before the code:
  - Enter in the Title field saves: the save call receives the field's value, as pressing Save does.
  - Enter while an IME composition is in progress (`isComposing` true) does not save. Japanese titles are typed this way.
  - Escape restores the value the field held when it took focus, does not close the panel or remove the editor, and leaves focus in the field.
  - A pasted value holding `\n` or `\r\n` ends up with each line break replaced by one space.
  - Focus leaving the field keeps the typed text.
- [ ] **Step 2: browser cases** at 1280×800 (the first column, and the last column of a full row: mount enough empty `li` tiles to fill the row) and at 390×844 (left and right column):
  - The fixture tells, asserted first: a 96-character title on one line at the field's font is wider than the tile.
  - At rest: the field's edges lie within its tile (the existing bound case covers this; keep it green).
  - Open (after `focus()` and a real click): the field is wider than the tile and at most about two tiles wide; its text wraps (`scrollWidth` ≤ `clientWidth` + 1) and it is taller than one line; its left and right edges lie within the page area's visible box and the window; `main`'s `scrollWidth` equals its `clientWidth`, and `document.documentElement.scrollWidth` ≤ `innerWidth`.
  - First column: its left edge lines up with the tile's left edge. Last column (1280) and right column (390): its right edge lines up with the tile's right edge.
  - Nothing moves: the Save button's and the next row's first tile's rects are the same before and after opening.
  - Above what is around it: `document.elementFromPoint` at a point inside the open field but outside its tile returns the field.
  - Closed again after focus moves to Save: it is back within its tile.
- [ ] **Step 3: Implement** the component change and the CSS; run node, jsdom and the whole browser project to green.
- [ ] **Step 4: Watched mutations**, each recorded: the leftward placement removed; the open size rule removed; the reserved at-rest space removed (so Save moves); the composition guard removed; Escape's restore removed; the line-break replacement removed; the stacking above neighbours removed.
- [ ] **Step 5:** `tsc -b`, `biome check`, the comment guard (`--base 09697925`), then commit `… (O274)`. Write `$E/T8-report.md`.

### Task 9: `docs/status.md` and `docs/notes.md` say the Title box opens (O274)

- [ ] **Step 1:** Read O274's row (~:1899) and its notes bullet (~:4641-4659). Make the minimal edits so every sentence is true after Task 8: the Title box opens downward, wider than its tile, while it is edited. Say in the report whether anything of the row's 1280 px half is left open once the box opens, quoting the row; do not close the row (the controller rules on that).
- [ ] **Step 2:** Grep `docs/` for other prose about the Title box's width or its sideways scroll and correct each hit the same way; list them.
- [ ] **Step 3: Two new open rows, the owner's (2026-11-13).** Add them after O281 in the open-items table, in the table's own shape (`| O282 | [ ] | **…** …`), and update every count of open items in `status.md` (the "leaving eighty-nine" sentence, "the eighty-nine open items tabled above", and any other count a grep for the number finds), so the doc check passes. Each row's substance, to be worded in the table's style, with no figure or claim beyond these:
  - **O282, notes on a phone.** A note's margin tab is placed outside the page box (`tabStyle` in `web/src/Annotations.tsx`: `left: 100%` or `right: 100%` plus a 4 px margin), so when the page fills a portrait phone's width the tab lies past the screen's edge; on 2026-11-13 the owner could not see it on an iPhone. By the code, not measured. And no touch control starts a note: `n` is the only route to `annotations:new` (`modules/annotations.ts`, `App.tsx`'s `moduleKey`). A finger reaches an existing note only through "Notes on this page", which shows only with Settings → Annotations → reveal ticked. The owner: not a priority now, kept so it is not lost.
  - **O283, shorter book URLs.** A book's URL carries its slug, made once from its title and never changed (`slugify` in `crates/taiyaku-library/src/import.rs`). A library made before afb6e4ba has slugs like `akame-ga-kill----v01--2015--cbz`; since then a title such as `Spy × Family - v01 (2020).cbz` becomes `spy-family-v01-2020`. The owner wants shorter URLs and asked to take this up sooner rather than later. Which form (the year dropped, or the volume under its series, such as `/read/<series>/v01`) and whether existing slugs change, with the old ones redirecting, are decisions not yet made.
  - Run the check that reads `docs/status.md` (find it in `scripts/check-all.sh`) and record its result.
  - *Amended after Task 9's review:* O282's notes report was folded into O139; the URLs row is O282 and the compact-tile row O283.
- [ ] **Step 4:** Commit `… (O274, O282, O283)`. Write `$E/T9-report.md`.

---

### Finish

1. **Whole-branch review** of `main..HEAD`, in two lanes:
   - (a) behaviour: Tasks 1 and 4;
   - (b) layout, CSS and docs: Tasks 2, 3, 5 and 6.

   Each lane reviews against the spec, the rulings and Review Focus above. One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.

   If the fix wave adds tests after Task 6, measure the node, jsdom and browser counts again and correct `docs/status.md` before the claim check.
2. **Final gate** on the final commit, at load < 24, by the Gate line above. Read the `.exit` file.
3. **The drive.** Serve a build from a scratch copy of `/home/davey/temp/manga-test` on 127.0.0.1:<port>, following the UI-prototype recipe: copy only `library.db`, and the account is `local`/`pw`.
   - Before anything else: check the port is free, then record the test library's `library.db` sha256 and `touch` a marker.
   - Never scan a library whose root is the test library.
   - Check each item with a real mouse at 390 and 1280 px, and under DevTools' touch emulation where a coarse pointer matters:
     - O229:
       - ⚙ → Account… opens the panel with Settings gone. A mouse click focuses ⚙ in Chromium, so the panel's Close puts focus back on ⚙.
       - `s` pressed on the page, then Account…, then Escape: focus goes back to the page, not ⚙.
       - Shift+A over Settings replaces it, with one dialog up. Shift+A from the page still opens the panel.
     - O171: the note editor's textarea computes 16 px under touch emulation and 14 px without.
     - O141: a second edition, made by the recipe's proposal accept and prefix rule, puts the edition key and a verdict in the readout. At 390 the row ends in "…" inside its padding; at 1280 it reads as before.
     - O172: a note's heading and its review row name the slot the readout names.
     - O274:
       - Give `local` `can_stamp`.
       - Add a scratch library whose root is a directory in the session scratchpad, holding one **copied** (not hard-linked) `.cbz` under a long name with no break, and scan only that root.
       - Press "What would this change?" and do not confirm.
       - At 390 the page does not scroll sideways.
   - Stop the server by PID and re-check the port.
   - Run `find /home/davey/temp/manga-test -newer <marker>` and take the sha256 again; both must show the library unchanged.
   - Delete the scratch copy in its own Bash call, by its literal absolute path.
4. **Optional, offered to the owner, not a gate: an iPhone check of O171.** iOS needs TLS, so serve the scratch build over the tailnet as the device-testing memory describes: `tailscale serve --bg --https=443 http://127.0.0.1:<port>`. Turn it off afterwards with `tailscale serve --https=443 off`, because it outlives the session.
   - A phone opens the editor only from an existing note (O139: Settings' note reveal, then the note list), so make one note at 1280 first.
   - The owner taps the text box and says whether the page zoomed. They can try ⚙ → Account… while there.
   - Read the request's user-agent before believing the result. The real iOS version is in `Version/`.
5. **Claim check** of the status edits, the spec and the PR body. Put the PR body in pass 1, and scope later passes to the correction commit.
6. **The PR body** at `$E/pr-body.md`, with the title as its first line, measured under 255 characters. Name Spec gap 1 and the owner's ruling on it, and the spec's amendment of O229's focus target. Ask the owner before pushing.

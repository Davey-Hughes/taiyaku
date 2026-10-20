# Sidebar and readability, PR 1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hold the prototype the user approved with tests and fix what spec §5 lists, so the sidebar is Libraries + Theme + More, the browse preferences live in View panels, search is a bar on the Library view, and text meets a 14px floor with Ink's colours lifted to APCA Lc 60 and its UI text at weight 500.

**Architecture:** The prototype is on the branch as the user tried it (`f85d07fa` … `62e76a9b`) and is the starting point: `AccountMenu.tsx` (More), `ViewMenu.tsx`, `popover.ts` (the shared close-on-Esc/outside/navigation hook), `SearchBar.tsx`, `searchHits.ts` (the debounce and sequence logic moved out of `Search.tsx`), `BrowseControlsContext` in `browseModule.ts`; `Search.tsx` is now only a redirect. Web only; no server change.

**Tech Stack:** React 19 + react-router 8, vitest (projects `node`, `jsdom`, `browser` in `web/vite.config.ts`) with the helpers in `web/src/testing/` (`mouse.ts` for real pointer input), biome.

**Spec:** `docs/superpowers/specs/2026-10-18-sidebar-readability-about-design.md` §1, §2, §3, §5, §6, §7. **Branch:** `sidebar-readability` in the main checkout `/home/davey/projects/taiyaku`, from `main` at `e4f2e4e5`. **Closes:** O221, and O223 (opened by this PR: main's Search box drops keystrokes typed fast). PR 2 (spec §4, About and licence) has its own plan.

## Global Constraints

- No server change. Paper's colours are unchanged. The reader looks as on `main`: in Ink with the bars up, `.reader-title-work` 600 `rgb(236, 228, 212)` 16px Shippori Mincho B1; `.reader-title-edition` and `.reader-readout` 400 `rgb(166, 155, 135)` 12px; `.reader-back` 400 `rgb(166, 155, 135)` 14px (census, below).
- The prototype's commits stay as they are; every fix goes on top.
- **A test is deleted only when the spec removes its behaviour**; the report names each deleted test with the spec line that removes it.
- **A ported test that fails on a behaviour assertion is a regression candidate:** do not change its expectation; report the test and what it expected, and stop for the controller.
- Every new test is watched failing first, against the defect it names, hand-applied where the code already passes; the report says how each was made to fail.
- Real-key browser tests focus the page first (a key press before anything has focus goes to the frame) and are also run alone with `-t`.
- Comments say why only: the reason, the invariant, what breaks; no history, no "the sidebar used to".
- Run each vitest project whole for any pass/fail claim: `pnpm -C web exec vitest run --project <p> --reporter=default >| <log> 2>&1`, then read the summary with `grep -a`. Never pipe a test command through `tail`. A browser run with import/connection errors and no failed assertion is the headless Chromium crash: check `coredumpctl list --since -1h`, rerun the whole project once.
- Lint with `pnpm -C web run lint` (one warning on `main`, `noDescendingSpecificity` at `admin.css:2078`; the branch's three new ones at `shell.css:84, 88, 352` must go). Typecheck with the script in `web/package.json`. Coverage has floors: `pnpm -C web run test:coverage` after deleting tests, and report any floor that moves.
- Never `cd` (it is zoxide here): absolute paths, `git -C`, `pnpm -C web`. Logs and scratch go in the session's scratchpad, never the working tree.
- Commits: one or more per task, the message says what the code now does; no attribution lines.

## Measured before planning

Census at `bbf39fe8` against `main` (`e4f2e4e5`), whole projects. `main` is green in all three once the fonts are reachable (a worktree with a symlinked `node_modules` makes Vite refuse the @fontsource files; the main checkout has no symlink). Lint exit 0 with 4 warnings (main 1); typecheck clean.

| File | Project | Failing | Cause | Task |
|---|---|---|---|---|
| `builtStylesheet.test.ts` | node | 15/22 | the reader pin redeclares 7 tokens as `light-dark(rgb…)`; PAIRED counts 20 not 13, each declared twice | 4 |
| `classNames.test.ts` | node | 1/6 | `browse-settings` is read only by the unused `BrowseSettings.tsx` | 2 |
| `styles/base.css.test.ts` | node | 1/10 | the two Ink weight selectors fail the guard's `:where(.library, .login)` prefix | 4 |
| `Search.test.tsx` | jsdom | 29/30 | `/search` is a redirect; debounce, sequence, stale-result and unmount tests cover kept behaviour | 3 |
| `Library.test.tsx` | jsdom | 1/43 | the O163 test expects the h1 first; it is inside `.page-head`, still the one hidden h1 | 3 |
| `Shell.test.tsx` | jsdom | 8/32 | Theme no longer last; three tests drive prefs in the sidebar; Account is More; no Search link; the phone search link's href is `/` | 1, 2, 3 |
| `Sidebar.test.tsx` | jsdom | 13/16 | Library and Search links gone; heading drawn with no library; links inside the closed More | 1 |
| `AcceptForm.browser.test.tsx` | browser | 1/5 | the root-scope line is "quiet by size", and `--text-meta` is now the size of `.consequence` | 4 |
| `Shell.browser.test.tsx` | browser | 1/16 | `.search-scope select` is gone | 3 |
| `markup.browser.test.tsx` | browser | 1/7 | "Proposals, 17 open" is inside the closed More | 1 |

Found by hand on the running servers: `main`'s Search box held "osi", "ohi", "oshi" over three fast typings of "oshi" (O223). The reader's computed values were identical on the prototype and `main`.

---

### Task 1: The sidebar, More and the Libraries link (spec §1)

**Files:**
- Modify: `web/src/Sidebar.tsx`, `web/src/AccountMenu.tsx`, `web/src/popover.ts`, `web/src/Shell.tsx` (the column and the drawer), `web/src/styles/shell.css`
- Test: `web/src/Sidebar.test.tsx`, `web/src/Shell.test.tsx` (its sidebar, Theme and More tests), `web/src/markup.browser.test.tsx`, `web/src/Shell.browser.test.tsx` (new real-mouse tests)
- Read first: status rows O163, O164 (the Proposals link's name), O220, O221; the style pass spec's drawer ruling (`docs/superpowers/specs/2026-09-17-style-pass-design.md:82`)

**Interfaces:**
- Consumes: the hook in `popover.ts` (read its signature; Task 2 uses it too).
- Produces: More as a disclosure — button with `aria-expanded` and `aria-controls`, the panel a list of links; for a non-admin a `NavLink` to `/account`. Task 2 builds View on the same hook and the same pattern.

- [ ] **Step 1:** Port `Sidebar.test.tsx` and the sidebar half of `Shell.test.tsx` to §1: the Libraries heading is a link to `/`, current only on `/` with no `?library`, drawn with no library; the list as before; Theme then More at the foot; Proposals, Configuration and Account reached by opening More (admin); a non-admin's More is a link. Delete only what §1 removes (the Library and Search links, the current mark on `/search`), naming the §1 line for each.
- [ ] **Step 2:** Add the panel's tests, each watched failing: Esc closes and returns focus to the button; a real-mouse click outside closes (`mouse.ts`); navigating closes; the button's badge and its accessible name carry the count in O164's words; More is in the accent on `/proposals`, `/config/*`, `/account` and not elsewhere; a non-admin's More has no `aria-expanded`, `aria-haspopup` or badge.
- [ ] **Step 3:** Change `AccountMenu.tsx` from `role="dialog"`/`aria-haspopup="dialog"` to the disclosure pattern (§1: the panel holds only links). Fix `markup.browser.test.tsx`'s Proposals-name test by opening More first, keeping its assertion.
- [ ] **Step 4:** Remove the three `noDescendingSpecificity` warnings at `shell.css:84, 88, 352` by ordering the rules, not by suppressing them.
- [ ] **Step 5:** jsdom, browser and node whole; lint (one warning, `admin.css:2078`); typecheck. The failures left are Tasks 2–4's rows of the census and no others.
- [ ] **Step 6:** Commit.

### Task 2: View panels (spec §2, View)

**Files:**
- Modify: `web/src/ViewMenu.tsx`, `web/src/Library.tsx`, `web/src/Series.tsx`, `web/src/browseModule.ts`, `web/src/settingsFields.tsx`
- Delete: `web/src/BrowseSettings.tsx` and the rules only it read; port `web/src/BrowseSettings.test.tsx`'s tests to a `ViewMenu.test.tsx`
- Test: `web/src/ViewMenu.test.tsx` (new), `web/src/Shell.test.tsx` (the three preference tests and "offers no switch for the libraries list"), `web/src/classNames.test.ts`, `web/src/Series.test.tsx`, `web/src/Library.test.tsx`

**Interfaces:**
- Consumes: Task 1's disclosure pattern and the `popover.ts` hook; `BrowseControlsContext` (read `browseModule.ts`).
- Produces: nothing new for later tasks; Task 3 places the search box before View in the Library page's top row.

- [ ] **Step 1:** Port `BrowseSettings.test.tsx` to `ViewMenu.test.tsx`, keeping each behaviour assertion: same preferences, same storage, Reset restores all five. Delete `BrowseSettings.tsx`; `classNames.test.ts` passes without its exemption.
- [ ] **Step 2:** Re-home `Shell.test.tsx`'s three preference tests onto the View panel of the page each control acts on (§2's table): Layout, Edition selector, Panel, Density, Reset on Series; Continue strip, Density, Reset on Library. "Offers no switch for the libraries list" asserts against the Library page's View, not the sidebar.
- [ ] **Step 3:** New tests, watched failing: each page's View holds exactly its row of §2's table, labels in sentence case; Reset from the Library page restores the Series page's three as well; View follows Task 1's Esc/outside/navigation rules (one test each, not a copy of Task 1's suite: assert it uses the shared hook's behaviour on this button).
- [ ] **Step 4:** Change `ViewMenu.tsx` to the disclosure pattern.
- [ ] **Step 5:** All three projects whole, lint, typecheck. Commit.

### Task 3: Search on the Library view (spec §2, search; O221, O223)

**Files:**
- Modify: `web/src/SearchBar.tsx`, `web/src/searchHits.ts`, `web/src/Library.tsx`, `web/src/Search.tsx` (redirect), `web/src/Routes.tsx`, `web/src/Shell.tsx` (the phone bar's search icon)
- Test: `web/src/Search.test.tsx` → port the kept tests to `web/src/searchHits.test.ts` and `web/src/Library.test.tsx`; `web/src/Shell.browser.test.tsx`; a new `web/src/SearchBar.browser.test.tsx`
- Read first: status rows O163, O173 (tab titles), O221; `Library.tsx`'s `emptyKind`

**Interfaces:**
- Consumes: `search(q, library)` from `web/src/library.ts:226`; the hook `searchHits.ts` exports (read it).
- Produces: none for later tasks.

- [ ] **Step 1:** Port `Search.test.tsx`'s debounce, sequence-number, stale-result and unmount tests to the hook and to the Library view, assertion for assertion. Delete the "Search in" select tests and the screen's own O163 h1 and O173 tab-title tests, naming §2's line for each; the Library view's title stands for `/?q=`. Fix `Library.test.tsx`'s O163 test to assert one visually hidden h1 wherever it sits.
- [ ] **Step 2:** New tests, each watched failing: with a query the hits replace the grid and the tabs, sort and continue strip are absent, and clearing restores all three; the scope follows `?library` and the placeholder names it; `/search?q=x&library=N` lands on `/?q=x&library=N` with hits; an account with no library (`status.libraries.length === 0 && status.restricted`) has no search box and sees `no-grant` (closes O221); the phone bar's search icon goes to the Library view, keeps `?library`, and focuses the box.
- [ ] **Step 3:** The O223 test, in the browser project with real keys: type a word with no delay between keys and assert the box and `?q=` hold every character. Watch it fail by drawing the box straight from `?q=` (`value={q}`, as `main`'s `Search.tsx` did), then pass with the box keeping its own text. Run it alone with `-t` and five times in a row.
- [ ] **Step 4:** Replace `Shell.browser.test.tsx`'s `.search-scope select` test with its §2 counterpart (the scope comes from the sidebar's library), or delete it naming §2 if nothing remains to assert.
- [ ] **Step 5:** All three projects whole, lint, typecheck, coverage. Commit.

### Task 4: Text — floor, Ink colours, Ink weight, the reader pinned (spec §3)

**Files:**
- Modify: `web/src/styles/tokens.css`, `web/src/styles/base.css`, `web/src/styles/admin.css` (the hard-coded `0.7rem` sites, and every `--text-meta` use that is a badge or chip → `--text-badge`), `web/src/styles/library.css`, `web/src/styles/shell.css` (badge uses)
- Test: `web/src/index.css.test.ts`, `web/src/builtStylesheet.test.ts`, `web/src/styles/base.css.test.ts`, `web/src/AcceptForm.browser.test.tsx`, a new `web/src/inkWeight.browser.test.tsx`
- Read first: status row O218 and the guard comments in `builtStylesheet.test.ts` and `styles/base.css.test.ts` (what each protects)

**Interfaces:**
- Consumes: the tokens as the prototype left them (spec §3's table).
- Produces: an `apcaLc(text, ground)` helper inside `index.css.test.ts`.

- [ ] **Step 1: APCA beside WCAG.** Add `apcaLc` to `index.css.test.ts` and assert, for every text token on bg, surface and field: Ink |Lc| ≥ 60; Paper's values recorded in the test's table as they are. APCA-W3 0.0.98G, which exists nowhere in the repo yet: for each colour `Y = 0.2126729·R^2.4 + 0.7151522·G^2.4 + 0.0721750·B^2.4` over sRGB channels in 0–1; if `Y < 0.022`, `Y += (0.022 − Y)^1.414`; if `|Yb − Yt| < 0.0005`, Lc = 0; normal polarity (`Yb > Yt`): `S = (Yb^0.56 − Yt^0.57)·1.14`, Lc = 0 if `S < 0.1` else `(S − 0.027)·100`; reverse: `S = (Yb^0.65 − Yt^0.62)·1.14`, Lc = 0 if `S > −0.1` else `(S + 0.027)·100`. Check it against two pairs: Ink `#a69b87` on `#15130f` gives |Lc| 48 (rounded) and Paper `#6b6151` on `#f3eee3` gives 70. Watch the floor fail by restoring `main`'s Ink muted. Also assert §3's other Ink rules: surface against bg ≥ 1.25:1 (WCAG); `--line` on surface no lower than `main`'s `--line` on `main`'s surface; `--line-strong` ≥ 3:1 on bg, surface and field; text on the accent ≥ 4.5:1. Watch the surface rule fail with `main`'s Ink surface.
- [ ] **Step 2: The reader's own values.** `index.css.test.ts` computes the reader's contrasts from the values the reader draws (the pin under `.reader-route, .reader`), not the lifted ones; assert they equal `main`'s pairs.
- [ ] **Step 3: The pin and the built stylesheet.** Make `builtStylesheet.test.ts` pass with what it guards intact (O218: `light-dark()` ships as written, each token's pair present). Either the pin stops redeclaring the shared token names, or the test learns that a scoped redeclaration under the reader is allowed — choose by which keeps O218's guard able to fail; report the choice and the mutation that shows the guard still fails.
- [ ] **Step 4: The weight rule's scope.** Make `styles/base.css.test.ts`'s guard pass with the rule written inside the guarded scope (or the guard widened only as far as the rule needs, with the reason in the test). Then `inkWeight.browser.test.tsx`, each watched failing: UI text is 500 under Ink chosen and under System with `prefers-color-scheme: dark` (emulated); 400 under Paper and under System with a light preference; titles keep their face's weight; the reader's values match the Global Constraints line under Ink.
- [ ] **Step 5: The floor.** Every remaining `--text-meta` or `0.7rem` site is classified as read text (`--text-meta`, 14px) or a badge/chip (`--text-badge`, 12px); the report lists each with its class. `AcceptForm`'s root-scope line stays quiet by colour, not size (it is read text, so it keeps the floor): the test asserts it is drawn in `--muted` where `.consequence` is in `--text`, and it is watched failing with the line set to `--text`.
- [ ] **Step 6:** All three projects whole, lint, typecheck, coverage. Commit.

### Task 5: Docs, the built app, and the gate (spec §6, §7)

**Files:**
- Modify: `docs/status.md`, `README.md` (line 319's list of routes, and any sentence naming the Search screen, the sidebar's links or the browse settings)

- [ ] **Step 1:** `docs/status.md`: the Later entry "A sidebar and readability redesign" is replaced by what this PR ships; O221 closed by the five-part edit; O223 opened and closed (main's Search box dropped keystrokes typed fast: "osi", "ohi", "oshi" over three tries of "oshi"); the four "Not in this spec" items join the Later list; piece 3 (grouping the Configuration screens, Stump motifs) recorded as next, and PR 2 (About) after it or beside it.
- [ ] **Step 2:** `README.md` for the new sidebar and search; nothing about About (PR 2).
- [ ] **Step 3:** `scripts/check-all.sh` into a log; quote its `all green` line and its exit status.
- [ ] **Step 4:** Build the app and look at it in both themes, desktop and 390px, beside a `main` build: the sidebar foot, both View panels, the search bar with and without a query, Configuration → Users and a folder page, and the reader. Screenshots go to the scratchpad and the report names what each shows.
- [ ] **Step 5:** Commit.

The controller then: an iOS device check of the phone search icon's focus from another screen (with the user; spec §2); a whole-branch review that reviews its own fixes; the claim check over the status rows, the README and the PR body.

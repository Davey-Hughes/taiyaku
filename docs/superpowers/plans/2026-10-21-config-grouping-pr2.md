# Configuration grouping, PR 2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Libraries, Users and Backup, and every page under them, on the four settings components and the look the user chose. On a wide screen the end of a settings tree opens beside its list, decided by the width alone. The prototype's switcher is gone, and the prototype's restructure is held by tests.

**Architecture:** The prototype's three commits are already on the branch, rebased onto the renamed `main`. They are kept, not rebuilt. `settingsLayout.tsx` holds `PageHeader`, `Section`, `Row` and `RemovalSection`. A new `SideBySide` component decides list, page, or the pair. It compares the screen's width with a hidden probe sized from the tokens, because a container query cannot read `var()`. Narrow, it does not mount the list. The pages' tests, written for the old markup, are re-pinned against the components.

**Tech Stack:** React 19 + react-router, CSS custom properties and container queries, vitest (`node`, `jsdom`, `browser` projects in `web/vite.config.ts`).

**Spec:** `docs/superpowers/specs/2026-10-20-config-grouping-design.md` §1–§4 and §6, for Libraries, Users and Backup (§0's PR 2). **Branch:** `config-grouping` in the main checkout `/home/davey/projects/taiyaku`, from `main` at `4ee86198`:

- `d224d46a` files O225 and O226.
- `49a0e712`, `7e24229e` and `a4e4492c` are the prototype's `a5337327`, `ded13af7` and `1f538bc9`, replayed with the rename. `git range-diff` shows only folder→directory/dir.

At `a4e4492c`, typecheck and lint pass. The node project has 3 failures and jsdom has 146, all from the prototype's restructure. The browser project has not been run.

## Rulings (the user, 2026-10-21)

1. **The list widens, as tried.** Beside an open page, the list's column is at least 20rem and grows to the table's 40rem measure where the window allows. Its rows draw in two lines while the column is under 36rem. The opened page starts one `--space-6` gap after the list, and only the width beyond 40rem + gap + 40rem goes to the page's right. §3 of the spec is corrected to say this in the same commit as this plan.
2. **The scan report's labels.** "Directories" (`walk.dirs`, every directory the walk recorded) becomes "Directories walked". "Directories removed mid-derive" (`derive.root_removed_mid_derive`, which counts **members** whose directory was removed during the derive, `derive.rs::write_roots`) becomes "Members whose directory was removed mid-derive". "Directory", "Directories excluded" and "Directories it could not read" stay as they are.

## Global Constraints

- **Headings:** Configuration's h1 is visually hidden (`Config.tsx`), so a page title is h2 and its sections are h3. Size comes from the component's class, not the tag.
- **Width:** every page here keeps the Configuration measure, `--config-measure` (40rem, set in `admin.css` on `.libraries, .users, .backup`).
- **The look (§2), already built:** label beside control, heading outside the box, bare box, `--line-strong` edge, `--line` between rows. A table in a box is inset `--space-4` on all four sides, and a selected row is lit inside that inset.
- **No setting gains or loses a function.** **Help text** may be shortened, moved beside its setting, or folded into a disclosure, but every fact stays. A task that rewrites help reports a table: each sentence of `main`'s help, and where its fact is now.
- **Words:** prose says "directory", names say "dir" (spec §5). No new "folder".
- **O175 (closed) holds:** a control's accessible name keeps its label's words in the label's own case. Where a control has an `aria-label`, it contains the visible label's words. `testing/fields.ts::fields` reads labels that wrap their control; a label tied by `for` must be read too (Task 8).
- **Re-pinned, not deleted (§6):** a test that pins old markup is rewritten against the components. Each task's report gives each test file's count before and after, and the counts are equal unless the report names what was added. A test named for a defect (an O-number, or "does not …" naming a past bug) is watched failing against that defect, hand-applied. The report says how.
- Every new test is watched failing first, against the defect it names. Assert per element: a whole-page `toContain` cannot see which text belongs to which row. Real-key browser tests focus the page first, and are also run alone with `-t`.
- Comments say why: the reason, the invariant, what breaks. No history, no "prototype", no "round".
- **Gates run whole projects:** `pnpm -C web exec vitest run --project <p> --reporter=default >| <log> 2>&1`. Read each summary with `grep -a`. Never pipe a gate through `tail`.
  - A browser run with import or connection errors and no failed assertion is the headless Chromium crash. Check `coredumpctl list --since -1h`, then rerun the whole project once.
  - O225's Filmstrip test fails now and then and passes on the rerun. If it is the only failure, rerun the project once and say so.
  - `pnpm -C web run typecheck`. `pnpm -C web run lint` adds no warning to `main`'s count; count `main`'s before Task 1.
- Never `cd` (it is zoxide here). Use absolute paths, `git -C` and `pnpm -C web`. The shell is zsh with `noclobber`: start a log with `>|`. Logs and scratch go in the session's scratchpad, never the working tree.
- Commits: one or more per task. The message says what the code now does. No attribution lines.

## Before Task 1

- [ ] Record the baseline. Read the forge's commit status for `4ee86198`, `main`'s CI (gitea MCP, or `curl` per the Gitea memory). Count `main`'s lint warnings. Run the browser project once on this branch into a log, and list its failing files and tests. That run is Task 8's starting list.

---

### Task 1: The components held, and the switcher gone (spec §1, §2)

**Files:**
- Delete: `web/src/ProtoSwitcher.tsx`
- Modify: `web/src/Config.tsx` (the import at `:30` and the mount at `:98-99`).
- Modify: `web/src/styles/settings.css`, the "PROTOTYPE ONLY" block from `:413` to the end:
  - Delete the banner, the "names" variant (`:422-432`), `.config:has(> .proto-switcher)` and the `.proto-switcher` rules.
  - The "full" rules lose their `:root[data-proto-list="full"]` prefix and join the split rules above.
- Modify: `web/src/Users.tsx:239` and `web/src/Libraries.tsx:415`. Drop the `split-panes` class, which no rule names. Task 2 replaces this markup.
- Modify: `web/src/classNames.test.ts` (the exemptions: `credentials` is no longer written, and `scanning` now has a selector) and `web/src/styles/base.css.test.ts` (index.css's import list now has `settings.css`).
- Create: `web/src/settingsLayout.test.tsx` (jsdom) and `web/src/testing/settingsRows.ts`
- Read first: `settingsLayout.tsx` (all of it, 175 lines); `testing/fields.ts`

**Interfaces:**
- Produces: `expectRowsWired(root: HTMLElement): void` in `testing/settingsRows.ts`. For every `.settings-row` under `root` that holds a form control, it checks two things:
  - The control's `labels` include that row's label.
  - Where the row has help, the control's `aria-describedby` names the help's id.
  It throws, naming the row, on a row whose control is unlabelled or undescribed. It also throws on a root with no rows, so a caller cannot check nothing. Tasks 3–6 call it on each page.

- [ ] **Step 1:** `settingsLayout.test.tsx`, one test per behaviour, each watched failing against its defect:
  - `PageHeader`: the back link's target and words; h2 by default and `level` when given; the description; the actions slot; `titleRef` makes the heading focusable (`tabIndex -1`).
  - `Section`: h3 by default and `level` when given; the description sits outside the box; the box holds its children; with no title, no heading.
  - `Row`: the label targets the control by `for`; a `check` row's checkbox toggles by a click on its label; the help's id is in the control's `aria-describedby`; no help means no `aria-describedby`.
  - `RemovalSection`: the box carries the danger edge's class and holds a confirm box rendered inside it.
- [ ] **Step 2:** `testing/settingsRows.ts`, with a test of its own: it passes on a wired row, and throws on an unlabelled row, an undescribed row and an empty root.
- [ ] **Step 3:** Delete the switcher and its CSS, and make the "full" rules unconditional. Fix the two node tests. The node project passes whole. Run jsdom whole and report its failure count (146 at `a4e4492c`, plus or minus what this task explains). Then typecheck and lint. Commit.

### Task 2: Side by side, decided by the width (spec §3, ruling 1)

**Files:**
- Create: `web/src/fitsPair.ts`, `web/src/SideBySide.tsx`, `web/src/SideBySide.test.tsx` (jsdom)
- Modify: `web/src/Users.tsx:230-290` and `web/src/Libraries.tsx:405-460` (the hand-built split becomes `<SideBySide>`).
- Modify: `web/src/settingsLayout.tsx:44-48` (the back link carries state, below).
- Modify: `web/src/styles/settings.css:345-446`:
  - The split rules key on `SideBySide`'s class, not on `@container split (width >= 62rem)`.
  - A token `--split-list: 20rem`.
  - The probe's rule.
  - The comment no longer says the list stays mounted hidden.
- Read first:
  - `Users.tsx:200-290` and `Libraries.tsx:355-460`: who gets the notices and `arrived` while a page is open.
  - `UserTable.tsx:58, 88-91` and `LibraryPage.tsx:462-470`: the row links, `aria-current` and `onOpen`.
  - `configPages.tsx:95-192`: `useLateAnswers`, `arrival`.
  - The jsdom setup files named in `web/vite.config.ts`: whether `ResizeObserver` is stubbed.

**Interfaces:**
- Produces: `useFitsPair(split: RefObject<HTMLElement | null>, probe: RefObject<HTMLElement | null>): boolean` in `fitsPair.ts`.
  - True when the split's content width is at least the probe's width. A probe of width 0 (unmeasured, or jsdom) never fits.
  - It measures in a layout effect and again on a `ResizeObserver` of both elements. Without `ResizeObserver`, it is false.
  - Tests elsewhere choose the layout with `vi.mock('./fitsPair')`.
- Produces: `SideBySide({ list, page }: { list: ReactNode; page: ReactNode | null })` in `SideBySide.tsx`.
  - It renders `.split`, holding the probe (`aria-hidden`, sized `calc(var(--split-list) + var(--space-6) + var(--config-measure))`, taking no space) and the panes.
  - No page: the list alone, at full width, as today.
  - A page that fits: the pair, list then page.
  - A page that does not fit: the page alone, with the list **not rendered**.
  - The list keeps one position in the tree, so opening the first row or switching rows never remounts it.
- Produces: `PageHeader`'s back link carries the router state `{ closed: <the page's own pathname> }`. When `SideBySide` renders with no page after a navigation whose state has `closed`, it focuses the link in its list whose `href` is that path, once per navigation. If no such link exists, focus is left alone.

- [ ] **Step 1:** `SideBySide.test.tsx`, with `useFitsPair` mocked, each test watched failing against its defect:
  - No page: the list.
  - Page and fits: both, and the list's DOM node is the same object across a page switch. Mark it.
  - Page and no fit: the page only, with no list node.
  - The back link's navigation focuses the matching row link, in both layouts.
  - A `closed` path with no matching row leaves focus alone.
- [ ] **Step 2:** `fitsPair.ts` tests in jsdom, with stubbed `clientWidth` and `getBoundingClientRect` and a stub `ResizeObserver` the test fires: wider fits, narrower does not, a zero probe does not, and no `ResizeObserver` means false. The real layout is Task 9's.
- [ ] **Step 3:** Build `fitsPair.ts`, `SideBySide.tsx` and the CSS. Ruling 1's columns use the tokens: `--split-list`, `--config-measure` and `--space-6`. The 36rem two-line rule on `.split-list` stays.
- [ ] **Step 4:** Move Users and Libraries onto `SideBySide`. A library's directories keep one route, so the library page stays mounted while directories are switched. Add these tests in `Libraries.test.tsx` and `Users.test.tsx`, in a new `describe` run with `-t` (the rest of those files is Tasks 4 and 5):
  - Narrow with a directory open: `libraryScanStatus` is called only by the directory page (once), never by the library page, and there is no directory table in the DOM.
  - Wide: the table, with `aria-current="page"` on the open row's link.
  - Users narrow with an account open: no table.
  Watch each fail against the prototype's hidden-list markup.
- [ ] **Step 5:** Typecheck, lint, the node project whole, and `SideBySide.test.tsx` plus the new `describe`s. Report jsdom's whole-project failure count against Task 1's. Commit.

### Task 3: An account's page re-pinned (spec §4, "An account")

**Files:**
- Modify: `web/src/AccountPage.test.tsx` (40 failing at `a4e4492c`) and `web/src/AccountPage.browser.test.tsx`; `web/src/AccountPage.tsx` only where a test finds a defect.
- Read first: `AccountPage.tsx:538-1000` (the sections), `git show 4ee86198:web/src/AccountPage.tsx` (the help text before the rewrite), and `git diff 4ee86198 HEAD -- web/src/AccountPage.tsx`.

- [ ] **Step 1:** Re-pin each failing test against the sections: Role & permissions, Libraries, Password, Sessions & keys, and the removal box. The last-admin reason is two sentences, one beside Role and one beside Remove…. The locked notice sits under the header.
- [ ] **Step 2:** Add one test calling `expectRowsWired` on the page, with and without an every-library grant.
- [ ] **Step 3:** Watch each defect-named test fail against its defect, and report how.
- [ ] **Step 4:** Report the help-fact table for the page. A fact the prototype dropped goes back, beside its setting.
- [ ] **Step 5:** `AccountPage.test.tsx` and `AccountPage.browser.test.tsx` pass, alone and in their projects. Report the counts before and after. Commit.

### Task 4: The Users screen and New account (spec §3, §4 "The Users table")

**Files:**
- Modify: `web/src/NewAccount.tsx`, moved onto `PageHeader` (‹ Users), `Section` and `Row`. Its `.actions` row goes, and the submit is the section's last row or the header's action, as fits the form.
- Modify: `web/src/Users.tsx:315-322`. The "No such account" fallback gets a `PageHeader` with its back link.
- Modify: `web/src/Users.test.tsx` (35 failing; it also covers New account) and `web/src/Config.test.tsx` (1 failing).
- Read first: `UserTable.tsx` (already on the components), `NewAccount.tsx`, `Users.tsx:200-330`.

- [ ] **Step 1:** Re-pin the Users and Config tests. The table is in an untitled box under a header "Users" with New account as its action. An account and New account open beside the table when wide (mock `useFitsPair`) and alone when narrow. Notices are drawn once: on the opened page while one is open, otherwise on the table.
- [ ] **Step 2:** Move New account onto the components, writing its help the way the prototype's pages are written. Report the help-fact table. Add `expectRowsWired` on New account.
- [ ] **Step 3:** Watch each defect-named test fail against its defect.
- [ ] **Step 4:** `Users.test.tsx` and `Config.test.tsx` pass, alone and in the jsdom project. Report the counts. Commit.

### Task 5: The Libraries table and New library (spec §3, §4)

**Files:**
- Modify: `web/src/LibraryTable.tsx`. The `h2` at `:54`, `p.new-item` at `:58` and the bare table at `:68` become a `PageHeader` ("Libraries", a one-line description, New library as the action) and an untitled `Section` holding the table, shaped like `UserTable.tsx`.
- Modify: `web/src/NewLibrary.tsx`. `p.back` at `:96`, the `h2` at `:99`, the `.field` labels and `.actions` at `:122` become `PageHeader`, `Section` and `Row`.
- Modify: `web/src/Libraries.tsx:513-519, :539-545`. The "No such …" fallbacks get a `PageHeader`.
- Modify: `web/src/Libraries.test.tsx` (35 failing).
- Read first: `UserTable.tsx` (the model), `LibraryTable.tsx`, `NewLibrary.tsx`, and `Libraries.tsx:380-560`.

- [ ] **Step 1:** Re-pin the Libraries tests. Clicking a library still changes the page (a library is not the end of its tree); the directory beside a library page is the side by side. Notices are drawn once: on the directory's page while one is open, otherwise on the library's page.
- [ ] **Step 2:** Move the table and New library onto the components, help written as in Task 4. Report the help-fact table. Add `expectRowsWired` on New library.
- [ ] **Step 3:** Watch each defect-named test fail against its defect.
- [ ] **Step 4:** `Libraries.test.tsx` passes, alone and in the jsdom project. Report the counts. Commit.

### Task 6: A library's page, a directory's page, and Backup (spec §4)

**Files:**
- Modify: `web/src/LibraryPage.tsx:412-441`. The rename form's `label.field` becomes a `Row`. Check that `:528`'s raw `<h4>` sits under an h3 section; it should, by the heading rule.
- Modify: `web/src/LibraryPage.test.tsx` (18 failing), `web/src/RootRow.test.tsx` (8), `web/src/DirPage.test.tsx` (5) and `web/src/Backup.test.tsx` (4).
- Read first: `git diff 4ee86198 HEAD -- web/src/LibraryPage.tsx web/src/RootRow.tsx web/src/DirPage.tsx web/src/Backup.tsx`.

- [ ] **Step 1:** Re-pin the four files. A library: header with ‹ Libraries, the name, the works-and-directories count, Scan library and Rename… as actions; Directories in a box with Add directory…; What the scan found; then the removal box, whose button reads Remove….
  - A directory: header with ‹ the library, the path as title, and the facts as description. Scan directory · Move… are the actions, and Move's box opens under the header. Then How it is scanned, How it is read, the scan report, and the removal box.
  - Backup: the description keeps the whole-database warning, password hashes included. Back up now is the action. Then the Backups section and the "How backups work" disclosure.
- [ ] **Step 2:** `expectRowsWired` on a library's page, a directory's page and Backup. Report the help-fact table for the directory page and Backup, whose help the prototype rewrote.
- [ ] **Step 3:** Watch each defect-named test fail against its defect.
- [ ] **Step 4:** Run the four files, and `RemoveLibrary`, `RemoveRoot` and `MoveRoot`'s tests, alone, then jsdom whole. Expect it green, or name what is left and why. Typecheck, lint. Commit.

### Task 7: The scan report's directory labels (ruling 2)

**Files:**
- Modify: `web/src/ScanReport.tsx:287, :361` and `web/src/ScanReport.test.tsx`.

- [ ] **Step 1:** Write a test holding each of the five directory labels to its field, per element: the `dt` inside `[data-field="dirs"]` and the others. Watch it fail on the two old labels.
- [ ] **Step 2:** Change the two labels. Grep `web/src`, `README.md` and `docs/*.md` (not dated specs or plans) for the old words, and report each hit.
- [ ] **Step 3:** `ScanReport.test.tsx` alone, then jsdom whole. Commit.

### Checkpoint: the user tries it

The controller serves the branch's built app per the live-server memory (`127.0.0.1:4399`, a scratch copy of the test library) and asks the user to look at:

- the Libraries table, New library and New account, which had no prototype round;
- the help rewrites;
- the pair at two wide widths, and the page alone when narrow.

Their rulings are recorded before Task 8.

### Task 7a: Rename in place, and no ring where focus only lands (checkpoint rulings)

The user, at the checkpoint (2026-10-21): the "No such account" copy stays, but its line draws a focus ring when the page is reached by typing its address ("kinda annoying"). Rename… "creates an element and shifts everything down": ruled, the title is edited in place. Everything else stays as built.

**Files:**
- Modify: `web/src/LibraryPage.tsx` (the header's actions `:375-390`, the rename form `:410-443`), `web/src/styles/settings.css` (`.rename-library > .settings-row`, `:340`), `web/src/styles/admin.css` (the `.rename-library` rules), and the stylesheet that rings `:focus-visible`.
- Modify tests: `web/src/LibraryPage.test.tsx` (its rename cases), `web/src/Libraries.shell.browser.test.tsx` (`:571` the rename refusal's rhythm, `:1068` the rename label at 320), `web/src/styles/settings.css.test.ts`, `web/src/styles/admin-config.css.test.ts:249`.
- Read first: `LibraryPage.tsx:280-445`; `settingsLayout.tsx` (`PageHeader`, `titleRef`); `shell.css:358-364` (the skip link's landing draws no ring); `Users.tsx:200-215, :310-325`; `Libraries.tsx:330-340` and its two "No such …" fallbacks.

- [ ] **Step 1: No ring where focus only lands.** An element given `tabIndex={-1}` so that the keyboard starts there or a screen reader announces it draws no focus ring, as `.library > main:focus` draws none. That is a settings page's title (`titleRef`), the "No such account / library / directory" lines, the gone line and `Libraries.tsx:337`. Controls keep their rings. One rule with one selector list, not a rule per element. A browser test loads a "No such account" address fresh (focus arrives with no pointer, so it is `:focus-visible`) and asserts the focused line draws no outline, and that the back link reached by Tab still draws its ring. Watch it fail with the rule removed.
- [ ] **Step 2: Rename edits the title in place.** Rename… swaps the page's `h2` for a text box in the same place, named "Name" (O175), holding the name with its text selected and focus in it. The header's Scan library · Rename… become Save · Cancel in the same row. Enter saves; Escape and Cancel close the editor and put focus back on Rename…; after a save, focus goes where today's form sends it. While the editor is open, a refusal is the alert drawn in the description line's place (the works-and-directories count). The box takes the title's font, size and line height, so the header keeps its height, and at narrow widths it fills the title's width. The rename form's `Row`, its label, and the CSS only it used go.
  - Every behaviour of today's rename is kept: what it sends, the name rules, the refusal sentences, the not-saved notice when the page has gone, and the table's name updating.
- [ ] **Step 3: Nothing below moves.** A browser test in the real shell measures the top of the first section (Directories) before Rename…, with the editor open, and with a one-line refusal shown, at 1280 and 390: equal within 0.5px. With the editor open at 320, nothing scrolls sideways. Watch it fail against today's form. Re-pin `:571` and `:1068` against the editor, or retire them into this test; the report names what each became.
- [ ] **Step 4:** Re-pin the jsdom rename tests and watch each defect-named one fail against its defect. Run `LibraryPage.test.tsx`, `Libraries.test.tsx` and `Libraries.shell.browser.test.tsx` alone, then jsdom and browser whole (browser: no failure outside the ledger's list). Typecheck, lint. Commit.

### Task 7b: Move… opens in a dialog (checkpoint ruling)

The user ruled that Move… gets the same treatment: nothing on a directory's page moves when it opens. Its box holds a library choice, a name and two disclosures, too much for a popover, so it opens in a modal `<dialog>`. That is the controller's call, and the user tries it.

**Files:**
- Modify: `web/src/RootRow.tsx` (`:691-700`, where `MoveRoot` is drawn under the header), `web/src/MoveRoot.tsx` (its outer element), and the dialog's CSS in `web/src/styles/settings.css`.
- Modify tests: `web/src/MoveRoot.test.tsx`, `web/src/RootRow.test.tsx`, `web/src/DirPage.test.tsx`, and any browser test that opens Move… (grep for it).
- Read first: `MoveRoot.tsx`; `RootRow.tsx:320-330, :590-700`; `Shell.tsx:186-198` (why the drawer is not a `<dialog>`; this one has no such reason).

- [ ] **Step 1:** Move… opens a native `<dialog>` by `showModal()`, labelled by a heading "Move {the directory's path}" (a long path breaks inside the dialog). It holds today's Move content unchanged: Move to, Its name, the two disclosures, Move and Cancel. The page behind stays where it was and is inert. Escape and Cancel close it and put focus back on Move…. While a move runs, Escape does not close it, as Cancel is disabled then. A finished move does what it does today. At 390 the dialog fits inside the page's gutter, and a dialog taller than the screen scrolls inside itself.
- [ ] **Step 2:** If jsdom's `HTMLDialogElement` lacks `showModal`, the browser tests carry the dialog's behaviour and the jsdom tests hold its content and wiring. Do not stub away a behaviour a test names.
- [ ] **Step 3:** Browser tests in the real shell: the first section's top is unchanged with the dialog open; Escape closes it and focus is on Move…; Escape while moving does not close it; nothing scrolls sideways at 390. Watch each fail against its defect.
- [ ] **Step 4:** The help is moved, not rewritten. Run the touched files alone, then jsdom and browser whole. Typecheck, lint. Commit.

### Task 7c: Rename moves nothing in any browser, and Move's dialog keeps the settings rhythm (Checkpoint 2 rulings)

The user, at Checkpoint 2 (2026-10-21): the "No such …" ring, Rename in place and a long name are good, and a finished move "seems correct". But "the highlight around the box to rename also shifts elements around", and in Move's dialog "the 'see what this would do' needs more spacing; in general the spacing of this box could be improved" (their screenshot: uppercase labels above the controls, the button touching the name box).

A measurement on the live server (`scratchpad/rename-shift/`, its `report-*.txt`) found three causes:
1. In Firefox the box is 37px tall where Chromium's is 32. Firefox floors a text input's line height at the font's `normal` (35px for the title's face at 24px), so `settings.css:66`'s `line-height: 1.25` has no effect, and everything below drops 5px. `minHeight` cannot cap it.
2. In every browser, Save · Cancel are about 71px narrower than Scan library · Rename…, so the title widens and the buttons move.
3. In the list column beside an open directory (407px at 1366), the box's 5px outset (`settings.css:60-61`) overhangs the column. That column is a scroll container (`settings.css:527-531`), so the box's sides and ring are clipped and a horizontal scrollbar appears.

**Files:**
- Modify: `web/src/LibraryPage.tsx` (the editor, ~`:380-520`, and the `titleHeight` measurement), `web/src/settingsLayout.tsx` (if the header's title or actions need a slot), `web/src/styles/settings.css` (`.page-title-box` and the header rules, the dialog rules), `web/src/MoveRoot.tsx` (its fields and buttons).
- Modify tests: `web/src/LibraryPage.test.tsx`, `web/src/Libraries.shell.browser.test.tsx` (the rename movement and 320 tests, the dialog tests), `web/src/MoveRoot.test.tsx`, `web/src/styles/settings.css.test.ts`.

- [ ] **Step 1: The box cannot move what is around it.** While editing, the `h2` stays in the flow, hidden, so it keeps the title's exact size whatever the name's length. The box is laid over the title's place, so its height and font metrics take no part in layout. The measured `minHeight` goes. The box stays inside the title's column, so it is never clipped and nothing scrolls sideways; the name's text may sit inset by the box's padding. The two button sets share one slot, for example stacked in a single grid cell with the idle set `visibility: hidden` and inert, so the slot is as wide as the wider set and swapping them moves nothing. Hidden controls are not focusable and not read.
- [ ] **Step 2: Hold it in a real browser.** The vitest browser project runs Chromium, which does not floor a text input's line height, so the movement test also runs with the box forced taller than the title (e.g. a style that sets its `line-height: 2`), as Firefox does. Measure every element of the header and the first section, not only Directories' top: the back link, the description line, each button, the Directories heading and box. Do it at 1920, 1366 with a directory open beside the library, and 390, with a one-line name and a wrapped name. All are equal within 0.5px with the editor open. The list column's `scrollWidth` equals its `clientWidth` with the editor open. Watch each fail against today's editor.
- [ ] **Step 3: Move's dialog in the settings rhythm.** Its fields become `Row`s in a box: Move to and Its name, label beside control, no uppercase. The dry run ("See what this would do"), Move and Cancel sit in a `.settings-actions` row with the rows' gap above it. The disclosures and the report keep the box's spacing. Content and behaviour are unchanged. `expectRowsWired` holds the dialog. A browser test holds a gap of at least `--space-3` between the last field and the buttons, per element.
- [ ] **Step 4:** Re-pin the affected jsdom and browser tests and watch each defect-named one fail against its defect. Run the touched files alone, then jsdom and browser whole (browser: no failure outside the ledger's list). Typecheck, lint. Commit.

### Task 7d: A library's address is its name, and New library leaves the name space (the user's rulings)

The user, 2026-10-21: "is it possible to change the path in the url to refer to the library name instead of the id number", then "fold it in" (into this PR). Ruled with them:
- **The exact name**, escaped only where an address needs it (`encodeURIComponent`). Browsers draw a space as a space. No slug.
- **Accounts too** (Task 7e). Directories keep their number: `/config/libraries/<name>/dirs/<did>`.
- **After a rename the page follows**: the address is replaced with the new name as the save lands. An old address then shows "No such library". The server remembers no past names.
- **The New pages move out of the name space**, so that no name is reserved: New library goes to `/config/new-library` (New account to `/config/new-account`, Task 7e). `/config/libraries/new` then means a library named "new".

**Files:**
- Modify: `web/src/Libraries.tsx` (routes `:395` "new" and `:409` ":id/*", and the name → library lookup), `web/src/Config.tsx` (a `new-library` route; the Libraries tab is the current tab there), `web/src/LibraryTable.tsx` (row links), `web/src/NewLibrary.tsx` (its back link; after making a library, where it navigates), `web/src/LibraryPage.tsx` (the rename replaces the address), and every other builder of `/config/libraries/…` (grep `web/src`; about a dozen files with tests).
- Read first: `Libraries.tsx:330-560`, `Config.tsx`, `SideBySide.tsx` (router state `{closed}` focuses the closed row: keep it working), and `crates/taiyaku-library/src/schema/016_library_names.sql` (library names are unique under `COLLATE NOCASE`, which folds ASCII case only).

- [ ] **Step 1: The lookup.** A library's segment is matched against the loaded list: an exact match first, else a match folding ASCII case only, as `NOCASE` does, which finds at most one. No match draws the existing "No such library" page. Links are always built from the library's current name. The API still takes ids; only the address changes.
- [ ] **Step 2: Names an address must escape.** Hold, in a real browser through the app's own router, that a name with a space, `/`, `?`, `#`, `%`, `&`, a non-ASCII letter (e.g. `É`) and a name that is all digits each opens its own page, its directory's page, and back to its row. Also hold that the server's page fallback answers these paths (`crates/taiyaku-serve`'s web fallback; read its test at `src/web/tests.rs:61`, which lists the paths it serves). Add such a path there if the fallback could treat it differently.
- [ ] **Step 3: Rename follows.** After a save, the address is the new name, with an open directory's `/dirs/<did>` kept. The entry is replaced, so Back does not return to the old name's address. The title's focus after a save is unchanged.
- [ ] **Step 4: New library at `/config/new-library`.** The Libraries tab is the current tab there. Its back link and Cancel go to the table. After making a library, it opens that library at its name's address, as it opens it today. Grep for every link to the old `new` address.
- [ ] **Step 5:** Re-pin every test that builds or reads these addresses; the report names the count per file and what each became. Tests for Steps 1–4, each watched failing against today's ids. Run the touched files alone, then node, jsdom and browser whole (browser: no failure outside the ledger's list). Typecheck, lint. Commit.

### Task 7e: An account's address is its name, and New account leaves the name space (the user's rulings)

The same rulings as Task 7d, for accounts. Account names are unique as written (`schema/001_initial.sql:204`, binary collation), so "Sam" and "sam" can both exist. The match is therefore exact first; a match folding ASCII case is used only when exactly one account matches it. Accounts cannot be renamed, so there is no rename to follow.

**Files:**
- Modify: `web/src/Users.tsx` (routes `:254` "new" and `:266` ":id"; the lookup), `web/src/Config.tsx` (a `new-account` route; the Users tab is current there), `web/src/UserTable.tsx` (row links), `web/src/NewAccount.tsx` (its back link; after making an account, where it navigates), and every other builder of `/config/users/…` (grep).
- Read first: `Users.tsx:200-330`, `SideBySide.tsx`, Task 7d's commit (its lookup and its `new-library` route are the model).

- [ ] **Step 1:** `/config/users/<name>` opens the account beside the table when wide and alone when narrow, as `:id` does today. No match draws "No such account".
- [ ] **Step 2:** New account at `/config/new-account` opens beside the Users table when wide, as it does today at `/config/users/new`, and alone when narrow. The Users tab is current there. Its back link carries `{closed}` as before; after making an account, it opens that account at its name's address, as today.
- [ ] **Step 3:** Names an address must escape, as in Task 7d Step 2, for accounts (a space, `/`, `?`, `#`, `%`, non-ASCII, all digits, and two accounts differing only in case).
- [ ] **Step 4:** Re-pin, test, run, and commit as in Task 7d Step 5.

### Task 7f: `.` and `..` are refused as names (the user's ruling)

The user, 2026-10-21: a library or an account named `.` or `..` can have no address, because the router and the URL parser collapse dot segments (`%2E` too), so the server refuses both names. That covers making a library, renaming one, and making an account, and a line says why. Only those two exact names are refused: `...`, `. `, `.a` and `a.b` stay valid.

**Files:**
- Modify: `crates/taiyaku-library/src/libraries.rs` (`create` `:110`, the rename's check `:157`; today both refuse only an empty name), the account's make path (`crates/taiyaku-serve/src/users_api.rs` and the store function it calls; grep where an account's name is checked), and their tests.
- Read first: how a refused name reaches the person today. NewLibrary.tsx, LibraryPage.tsx's rename and NewAccount.tsx each draw a refusal; find whether they show the server's sentence or their own, and keep to that.

- [ ] **Step 1:** The store refuses `.` and `..` on each of the three paths, with a test per name per path, each watched passing the name through before the check.
- [ ] **Step 2:** The person reads why: "An address cannot hold “.” or “..” as a name." on each of the three pages, drawn where that page draws a refused name today. Test it per page in jsdom.
- [ ] **Step 3:** Existing rows are left as they are, with no migration. Count such names in the scratch copy of the test library's database and give the count in the report.
- [ ] **Step 4:** Run the touched crates' tests (`--test-threads=8`), jsdom whole, typecheck and lint. Commit.

### Task 7g: every wait is drawn only once it has lasted 200 ms (the user's ruling)

The user, 2026-10-21: as Rename's Save draws "Saving…" only for a save still out after 200 ms (`useDrawnWait`, `web/src/hooks.ts:108`, used at `LibraryPage.tsx:169`), so do the other waits.

**Files:** `MoveRoot.tsx` (`:285` "Working out what this would do…", `:341` "Moving…"), `RemoveRoot.tsx:202`, `RemoveLibrary.tsx:165` and `AccountPage.tsx:1013` ("Removing…"). Grep the screens in this PR for any other drawn wait (a label ending "…ing…").

- [ ] **Step 1:** At each site the wait's label is drawn only for a wait still out after 200 ms. The control keeps the width it will need, as Save does. The hold (disabled controls, refused Escape, second submits) starts at the send, as it does today.
- [ ] **Step 2:** Per site: an answer inside 200 ms never draws the label, and a slower one draws it from 200 ms. Each is watched failing with the site's old drawing. Where a browser test pins a label's width, run it at both labels.
- [ ] **Step 3:** Run the touched files alone, then jsdom and browser whole, typecheck and lint. Commit.

### Task 7h: a name that clashes says so as it is typed (the user's rulings)

The user, 2026-10-21, after trying the rename's refusal: "when typing in the library name if it collides it should show immediately instead of waiting to submit". Ruled with them:
- **Three boxes:** the rename box on a library's page (`LibraryPage.tsx`), New library's name (`NewLibrary.tsx`) and New account's name (`NewAccount.tsx`).
- **A clash, as the store judges it.** For a library: another library's name equal after trimming (`trim_name`, `crates/taiyaku-library/src/libraries.rs:80`) and folding ASCII case only (`COLLATE NOCASE`, as `byName` in `web/src/configAddress.ts` already folds). The library's own current name is not a clash in its rename. For an account: another account with exactly that name (binary collation, stored as written), so "Sam" and "sam" do not clash. `.` and `..` (for a library, after trimming) show at once too.
- **What it says:** the server's own sentence for that refusal (a taken library name is "a library is already called `<name>`", `libraries.rs:190`; find the account's and the dot names' (`NO_ADDRESS`) sentences). Drawn where a refused save of that box draws today: the rename's callout, laid over and moving nothing, and New library's and New account's refusal place.
- **While it clashes, the box's Save, Make the library or Create account is held:** the held (dashed) look at once, not after 200 ms, because this is not a wait. A press and Enter in the box do nothing. The hold lifts the moment the name stops clashing. The server stays the authority: a list gone stale (another admin's rename) still ends in the server's refusal, drawn as today.

**Files:** the three components and the lists they can read (the libraries list `Libraries.tsx` holds, the accounts list `Users.tsx` holds), `settings.css` for the held look if it is not already shared, and tests beside each.

- [ ] **Step 1:** One small pure function per kind (library, account) answers "does this name clash, and with what sentence", with node tests: trimming, ASCII case only (`É` against `é` does not clash), the own name, `.` and `..`, the exact account rule.
- [ ] **Step 2:** Each box shows the sentence as it is typed and holds its button. Tie the sentence to the box (`aria-invalid`, `aria-describedby`), and announce it once when it appears (a polite live region), not on every keystroke. jsdom tests per box: a clash in another case shows at once and holds; typing on clears both; Enter does nothing while held; the rename's own name does not clash.
- [ ] **Step 3:** In a real browser: typing a clash into the rename box moves nothing on the page (as the callout's tests measure), and clearing it moves nothing.
- [ ] **Step 4:** Run the touched files alone; then node, jsdom and browser whole, typecheck and lint; then `scripts/check-all.sh`, updating `docs/status.md`'s web test figures from the run. The status entry and the spec's rename section say a clash shows as it is typed. Commit.

### Checkpoint 2: the user tries Rename and Move…

The controller rebuilds into the same scratch `dist`; the user reloads and rules before Task 8.

### Task 8: The browser tests that pin the old markup (spec §1, §6)

Dispatched in two parts, each sized to finish in one implementer session: **8a** is Step 2's `fields()` and every failing file but `Config.browser.test.tsx`; **8b** is `Config.browser.test.tsx`. Each runs Steps 1, 3 and 4 over its own files.

**Files:**
- Modify: the failing files from the run before Task 1, re-run now. Expect `Config.browser.test.tsx` (e.g. `:1659`, `:1872 .user-row`), `fieldNames.browser.test.tsx` (`:505-518`, the libraries pages' h2/h3 outline; and O175's "names the add-account form's controls as written", red since Task 4 because its precondition reads `label.field`, which New account no longer draws) and `Libraries.shell.browser.test.tsx`.
- Modify: `web/src/testing/fields.ts::fields`. It reads a label tied to its control by `for` as well as one that wraps it, so `fieldNames.browser.test.tsx`'s O175 check reaches every row.

- [ ] **Step 1:** Run the browser project whole and list what fails.
- [ ] **Step 2:** Extend `fields()`, with a test of its own: a `for` label and a wrapping label are both read; a `for` naming no control throws.
- [ ] **Step 3:** Re-pin each failing test. In `fieldNames.browser.test.tsx`, the password box's name is now "New password", its visible label (Task 3), and the settings rows' labels have no `text-transform`, so a precondition that a label is drawn in capitals no longer holds; re-pin what O175 needs, a name in the label's own case. Its outline covers each page in this PR: Configuration's h1, then the page title as h2, then its sections as h3.
- [ ] **Step 4:** Watch each defect-named test fail against its defect. Run the browser project whole and report the counts per file. Commit.

### Task 9: Side by side in a real browser (spec §3, §6)

Dispatched in two parts, each sized to finish in one implementer session: **9a** creates `SideBySide.browser.test.tsx` with Steps 1, 2, 3, 4 and 5 and their defects in Step 7; **9b** adds Step 1b (classic scrollbars), Step 6 (no sideways scroll at 390 in Paper and Ink), and the headings outline of `fieldNames.browser.test.tsx` read at a wide width, where the pair is drawn (a library page beside a directory's, the Users table beside an account).

**Files:**
- Create: `web/src/SideBySide.browser.test.tsx`, modelled on `Libraries.shell.browser.test.tsx`'s `shell(width, height, entry)` (`:217-250`), which mounts the real `Shell` so the sidebar and `main`'s padding decide the width.
- Read first: `Config.browser.test.tsx:261-350` (`routed`, `settled`, `inShell`, `assertNoSidewaysScroll`, `sized`) and `About.browser.test.tsx:556-558` (setting the theme).

- [ ] **Step 1:** At two wide widths (e.g. 1366 and 1920):
  - The pair is drawn for an account and for a directory.
  - The open row has `aria-current="page"`.
  - The gap between the list's right edge and the page's left edge equals `--space-6` at both widths.
  - The list is at least 20rem and at most 40rem.
- [ ] **Step 1b:** With classic scrollbars shown (the browser project hides them by default; launch without `--hide-scrollbars`, or drive the built app in a browser that shows them) at a window within one scrollbar's width above the threshold, with a list taller than the window and a page shorter than it, the layout does not change over a second. Watch it fail with the scrollbar gutter fix removed.
- [ ] **Step 2:** At a narrow desktop width under the pair's threshold and at 390, the page is drawn alone, with no list in the DOM. With a directory open, `libraryScanStatus` is called once, by the directory page's own `useOtherScan` (`DirPage.tsx`), and never by the unmounted library page. The fixture has no other directory of that library scanning, since `useOtherScan` asks again on every `SCAN_POLL_MS` while one is.
- [ ] **Step 3:** Focus. Opening a row by a click puts focus on the opened page's heading, and the back link returns it to that row's link, in both layouts. Run with real clicks and keys, with the page focused first.
- [ ] **Step 4:** The list survives a row switch: an expando on its DOM node is still there, and its scroll position is kept.
- [ ] **Step 5:** A selected last row ends at least `--space-4` above its box's inner bottom edge.
- [ ] **Step 6:** At 390, in Paper and in Ink, no page in this PR scrolls sideways: the Users table, an account, New account, the Libraries table, New library, a library, a directory, and Backup.
- [ ] **Step 7:** Watch each test fail against its defect, hand-applied:
  - the list mounted hidden;
  - a literal breakpoint 100px off;
  - a gap that grows with the width;
  - no focus return;
  - the list keyed per row;
  - the table's inset removed.
  Run the file alone, then the browser project whole. Commit.

### Task 10: Docs and the gate (spec §6)

Dispatched in two parts, each sized to finish in one implementer session: **10a** is Steps 1, 2 and 4, with the dead-rule sweep and the hand-offs earlier reviews left for it; **10b** is Step 3, the by-eye pass on the built app, which reports what it sees and changes nothing. Since this plan was written, Tasks 7d–7g changed what the docs describe: addresses are names (`/config/libraries/<name>`, `/config/users/<name>`, `/config/new-library`, `/config/new-account`), `.` and `..` are refused as names (Rust, in `taiyaku-library` and `taiyaku-serve`), and every wait on these screens is drawn only once it has lasted 200 ms. The status entry and the spec say so, and the README too where it names an address.

**Files:**
- Modify: `docs/status.md`. The Later entry "Next: group the Configuration screens" (`:5539-5550`) says PR 2 shipped and what it holds, and that PR 3 is next. Re-measure the web test counts there from the node and jsdom runs; do not add to them. Update the `web/` row of What is built where it describes the Configuration screens.
- Modify: `README.md`, where it describes the Configuration screens' layout. Grep it; it may say nothing.
- Modify: `web/src/styles/admin.css`. Remove the rules the moved screens left matching nothing (`.add-user`, `.new-account`, `.user-row`, and any others a grep of each selector against `web/src` finds dead), and the comments that justify one rule by a dead one (e.g. `:1349-1351`, `:2525`, `:2541` at `62b308db`). Report each selector removed and the grep that showed it dead.

- [ ] **Step 1:** Docs as above.
- [ ] **Step 2:** Run `scripts/check-all.sh` into a log. Quote its `all green` line and exit status. Report any coverage floor that moves.
- [ ] **Step 3:** By eye, on the built app (production CSS differs from dev, O218), in both themes at 1920, 1366, 1024 and 390, on every page in this PR. Put screenshots in the scratchpad, and say what each one shows.
- [ ] **Step 4:** Commit.

### Task 10c: what the look-over found, part one (the user's rulings and the controller's triage)

Task 10b's look-over of the built app (`.superpowers/sdd/task-10b-report.md`, findings F1–F17, with screenshots) found these. Each item is fixed, then held by a test watched failing against the defect as the look-over saw it.

- **F1 + F16, a rename's refusal (the user's ruling):** it is drawn as a refusal, in the look the other refusals on these screens use, not as plain text in the description's place. While the editor is open, room for a two-line answer is kept under the title, so that a refusal moves nothing below it at any width. At 390 today it wraps to two lines and pushes the page down 21px. Hold it at 1920 and 390, with a refusal long enough to take two lines at 390.
- **F3, the Move… dialog in Ink (the user's ruling):** in Ink the dialog sits on a slightly lighter surface than the page, as a raised panel does, so that it stands off the darkened backdrop. Today the backdrop darkens the page to the dialog's own colour. Paper is unchanged. Use the tokens' existing raised surface if there is one (`web/src/styles/tokens.css`). Hold that the dialog's background differs from the backdrop's darkened page in Ink by a measured amount, and that Paper's is unchanged.
- **F4, separator dots:** no line starts with a "·" left over from a wrap ("· Last seen:", "· Last scan:" on two-line list rows: Users, Libraries, a library's Directories), and no line ends with a lone "·" (the directory page's description). Find how the separators are drawn and keep each with the item it follows. Hold it at the widths where the look-over saw it (Users pair at 1366, 390).
- **F5, paths in titles:** a long path in a page title (the directory page's) and in the Move… dialog's title breaks after `/` and `-`, as the tables' paths do, and never inside a word. Reuse whatever the tables use. Hold it at 1920 and 390 with a path that has no space.
- **F7, the Users pair's headings:** beside the Users table, the account page's heading lines up with the list's, as the library page's and the directory page's do (both at y≈114 today; the Users pair's are ≈85 and ≈114, because the account page keeps its "‹ Users" back link above its heading). Follow what the library|directory pair does with its back link. Hold the two headings' tops equal within 0.5px at 1920 and 1366, for both pairs.

**Files:** find them from the report's page names. Read first: `task-10b-report.md` and the screenshots it names (under the look-over's scratchpad `shots/`).

- [ ] **Step 1:** Fix each item; its test watched failing first.
- [ ] **Step 2:** Run the touched files alone; then node, jsdom and browser whole, typecheck and lint. Commit (one commit per item is fine).

### Task 10d: what the look-over found, part two

- **F8, the scrollbar gutter:** every Configuration tab keeps the same gutter, so that moving between tabs moves nothing (today the nav's underline ends at x=1881 on the side-by-side screens and at 1896 on Libraries, New library, Backup and Rules, at 1920). Reserve it wherever any Configuration screen draws, keeping the reason it exists (the layout must not flip near the pair's threshold). Hold the nav underline's end equal across all four tabs and their pages at 1920 with classic scrollbars, which the second Chromium launch draws (`SideBySide.scrollbars.browser.test.tsx`).
- **F11, New library's Browse…:** the button and the path box beside it share a height and a top (today 33px against 30px, tops 3px apart, at every width). Hold the tops and heights equal within 0.5px.
- **F12, a grant's checkbox with a mouse:** the box lines up with the library's name (its first line), not with the middle of the whole item. Today it sits about 4 lines below the name at 1920 and 9 at 390. Hold the box's centre within the name's first line box at 1920 and 390.
- **F13, a grant under a coarse pointer:** the gap from a library's name to its first path is the same whatever the name's length (today about 35px under a one-line name and about 25px under a two-line one). Keep the 44px band. Hold the gap equal within 1px for a one-line and a two-line name.
- **F14, unchecked boxes and radios in Ink (the user chose to fix it here):** an unchecked checkbox's and radio's edge has at least 3:1 contrast against the surface it sits on (WCAG 1.4.11), in Ink and in Paper. Today Ink draws them grey on grey: fill rgb(59,59,59), edge rgb(99,99,99). Use tokens. Check whether main draws them the same, and say so in the report. Hold the contrast with a computed-style measure.
- **F15, the backup's file link (the user chose to fix it here):** the backup list's file name takes the look of the names in the tables (bold, no underline at rest), and keeps a visible mark on hover and focus as they do. Check whether main draws it the same.
- **F17, times in UTC (the user chose to fix it here):** "Last seen" and "Last scan" say their times are UTC, as the Backups list labels its own ("UTC"). Use the same form Backup uses. Check whether main draws them the same.

- [ ] **Step 1:** Fix each item; its test watched failing first.
- [ ] **Step 2:** Run the touched files alone; then node, jsdom and browser whole, typecheck and lint; then `scripts/check-all.sh`, and update the status entry's counts from the run if they move. Commit.

The controller then runs:

- the mutation sweep (`scripts/mutants.sh --since main`) over the Rust Task 7f changed;
- a whole-branch review that also reviews its own fixes;
- the claim check over the status entry, the spec's §3 correction and the PR body.

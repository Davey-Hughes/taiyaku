# Configuration grouping, PR 3 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rules, `/account`, About and Licence are drawn on the four settings components and the look PR 2 gave Libraries, Users and Backup. The reader's account panel keeps its look. A new API key is announced without reading it aloud (O228). The waits on these screens are drawn at 200 ms, and a control a wait holds while it has focus keeps that focus.

**Architecture:** The components already exist in `web/src/settingsLayout.tsx`, styled in `web/src/styles/settings.css`. Each screen moves onto them, and the tests that pin its old markup are re-pinned. `Section` gains one prop, `titleId`, because Licence's contents links land on its headings. `Account.tsx` draws one fragment for two placements, so the page's sections move onto the components and the panel's markup stays as it is. Nothing here touches Rust or side by side.

**Tech Stack:** React 19 + react-router, CSS custom properties and container queries, vitest (`node`, `jsdom` and `browser` projects in `web/vite.config.ts`).

**Spec:** `docs/superpowers/specs/2026-10-20-config-grouping-design.md`: §1, §2 and §4's closing paragraphs ("The rest follow the same pattern" to "Help text") and §6, for Rules, `/account`, About and Licence (§0's PR 3). **Branch:** `config-grouping-3` in the main checkout `/home/davey/projects/taiyaku`, from `main` at `a4c58019`.

## Rulings

1. **O228 (the user, 2026-10-25): announce the key.** When Create makes a key, a polite live region says so without the secret, e.g. "Key made: <name>. Copy it now…". Create keeps focus, as Change password does when it says "Password changed". This holds on `/account` and in the reader's account panel.
2. **Held controls keep focus (the user, 2026-10-23; spec §4).** O228's row applies this to `/account`'s Change password and Create. §4 applies it to every in-scope page, so Rules' Save, Import and the preview's buttons follow it too.
3. **Waits are drawn at 200 ms (the user, 2026-10-21; spec §4).** A save is drawn waiting only once it has lasted 200 ms, and what it holds is held from the send.

## Design calls (mine, for the user's checkpoint)

- **Rules:**
  - The page's bare `<h2>Rules</h2>` becomes `PageHeader` with no description.
  - The list pane and the rule pane each sit in an untitled box (`Section` with no title), so no new headings are invented. The preview sits below them in a box titled "Preview", the name its tablist already has.
  - The save-derive disclosure and the action row (Save, Import with its help and file input, Export) stay at the foot, after the boxes. The action row has help text and a hidden input, so it does not fit `PageHeader`'s actions slot.
  - RuleForm keeps its `.field` labels, since §1 says only Rules' headings and boxes change.
- **`/account`:**
  - `PageHeader` at level 1, titled "Account", with no back link.
  - Sections Password, API keys and Signed in, at h2.
  - Password: rows "Current password" and "New password" (PR 2 renamed the account page's box "New password" the same way). The key box gains a visible label, "New key", and keeps its placeholder. The API keys sentence becomes that section's description, so the order stays: the sentence, then the address, then the form.
  - Sign out stays at the foot of Signed in.
- **About:**
  - `PageHeader` at level 1.
  - Sections This build, Formats and Licence. This build keeps its `dl`, drawn with the rows' rhythm, since its term–definition pairs are what a screen reader reads. Formats is a table in a box.
- **Licence:**
  - `PageHeader` at level 1, with back link ‹ About.
  - The contents `nav` sits under the header. Each licence is a `Section` whose heading carries the fragment's id.

## Global Constraints

- **Headings:**
  - Configuration's h1 is visually hidden (`Config.tsx`), so Rules' title is h2 and its section is h3.
  - `/account`, About and Licence keep a visible h1 title (`PageHeader level={1}`) and have h2 sections (`Section level={2}`).
  - Size comes from the component's class, not the tag.
  - Rules' DeriveSection is a report drawn under whatever heading holds it (`ScanReport.tsx`, `ReportLevel`). Its level follows the heading it sits under.
- **Width:**
  - `/account`, About and Licence keep the Configuration measure: `--config-measure`, 40rem, set on `.account, .about` in `admin.css`.
  - Rules keeps its own width. `--config-measure` is not defined inside `.config`, so `.page-header` and `.settings-section` there have no max-width.
  - Task 4 pins this with a test, so a later definition cannot shrink Rules unseen.
- **The look (§2), already built:** label beside control, heading outside the box, bare box, a `--line-strong` edge, `--line` between rows. A table in a box is inset `--space-4` on all four sides.
- **The reader's account panel keeps its look.** `Account.test.tsx`'s panel cases (`:305-439`) and `Account.browser.test.tsx`'s panel cases (`:119-336`) pass with no edit to what they assert. Task 3 changes what the panel does: the announcement and the held look. Its report names each panel assertion it touched, and why.
- **No setting gains or loses a function.** Help text may be shortened, moved beside its setting, or folded into a disclosure, but every fact stays. A task that rewrites help reports a table: each sentence of `main`'s help, and where its fact is now.
- **Held controls (Ruling 2):**
  - A control a wait or hold takes while it has focus is held by `aria-disabled`: through `HeldButton` (`web/src/HeldButton.tsx`) for a button, or as `AccountPage.tsx`'s checkboxes are held (O159) for anything else.
  - A press, a choice or an edit of a held control is turned down.
  - A held control is drawn as a `disabled` one is, under forced colours too.
  - A control that is disabled from its first draw (a session's Revoke for the current session) stays `disabled`.
- **Waits (Ruling 3):**
  - `useDrawnWait` (`web/src/hooks.ts`) draws the wait's words and held look after 200 ms.
  - The handler's own guard holds presses from the send.
  - A button that waits keeps the box of the wider of its two words (`WaitWords`, `.keeps-wait-width`), so drawing the wait moves nothing.
- **Words:** prose says "directory" and names say "dir". No new "folder".
- **O175 (closed) holds:** a control's accessible name keeps its label's words in the label's own case. Where a control has an `aria-label`, it contains the visible label's words.
- **Re-pinned, not deleted (§6):**
  - A test that pins old markup is rewritten against the components. Each task's report gives each test file's count before and after. The counts are equal unless the report names what was added or why one was removed.
  - A test named for a defect (an O-number, or "does not …" naming a past bug) is watched failing against that defect, hand-applied. The report says how.
- **New tests:**
  - Every new test is watched failing first, against the defect it names.
  - Assert per element: a whole-page `toContain` cannot see which text belongs to which section.
  - Real-key browser tests focus the page first, and are also run alone with `-t`.
- **Comments say why:** the reason, the invariant, what breaks. No history, and nothing about cards that are gone. The stale card comments are `About.tsx:7-11`, `Licence.tsx:11-16, :41-44, :84`, `Account.tsx:43-50, :61-74`, and `admin.css:1970-1990, :2077-2083, :2112-2115`. Each is rewritten by the task that moves its screen.
- **Dead CSS:** a task that moves a screen deletes the rules it leaves matching nothing. It reports each selector deleted and the grep against `web/src` that showed it dead. A rule the panel still uses (reader-*) is not dead.
- **Gates run whole projects:**
  - Run `pnpm -C web exec vitest run --project <p> --reporter=default >| <log> 2>&1`, and read each summary with `grep -a`. Never pipe a gate through `tail`.
  - A browser run with import or connection errors and no failed assertion is the headless Chromium crash. Check `coredumpctl list --since -1h`, then rerun the whole project once.
  - O225's Filmstrip test fails now and then and passes on the rerun. If it is the only failure, rerun the project once and say so.
  - Also run `pnpm -C web run typecheck`. `pnpm -C web run lint` adds no warning to `main`'s count.
- **Shell:**
  - Never `cd` (it is zoxide here). Use absolute paths, `git -C` and `pnpm -C web`.
  - The shell is zsh with `noclobber`, so start a log with `>|`.
  - Logs and scratch go in the session's scratchpad, never the working tree.
- **Commits:** one or more per task. The message says what the code now does. No attribution lines.

## Before Task 1

- [ ] Record the baseline:
  - The forge's commit status for `a4c58019` (gitea MCP, or `curl` per the Gitea memory).
  - `main`'s lint warning count.
  - The node, jsdom and browser projects run once on this branch, each into a log, with their totals and any failing tests. The ledger records these.

---

### Task 1: About and Licence on the components, and a landing heading for `Section` (spec §1, §4)

**Files:**
- Modify: `web/src/settingsLayout.tsx` (`SectionProps` `:139-161`, `Frame` `:178-207`) and `web/src/settingsLayout.test.tsx`.
- Modify: `web/src/About.tsx` (130 lines) and `web/src/Licence.tsx` (238 lines): the cards (`section.about-card > h2.about-heading`), Licence's hand-drawn `p.back`, the h1s.
- Modify: `web/src/styles/admin.css`:
  - `:1970-2122`: the `.account, .about` measure stays; the `.about-*` and `.licence` rules.
  - `.config-table` inside About (`:1636-1760`). `.settings-box > .config-table` (`settings.css:922-933`) already covers a table in a box.
  - The coarse-pointer rules for `.about-licence a` and `.licence .back` (`:2519`, `:2615-2632`).
- Modify tests: `About.test.tsx` (~11), `About.browser.test.tsx` (~9), `Licence.test.tsx` (~11), `Licence.browser.test.tsx` (~7). In `Shell.browser.test.tsx`: Licence's back link at 44px (`:1131`), and the contents cases (`:1278-1420`), which read `heading.closest('.about-card')`. Also `Routes.test.tsx` (`:330-392`) if its landmark selector changes, and `fieldNames.browser.test.tsx`'s heading outline (`:602-715`).
- Read first: `settingsLayout.tsx` (all of it), `Licence.tsx:186-205` (the fragment focus and scroll), `Backup.tsx` (a PR 2 page with a header action and a section).

**Interfaces:**
- Produces: `Section` and `RemovalSection` take `titleId?: string`, allowed only with `title`. When given, the heading carries that `id` and `tabIndex={-1}`, so a fragment link can move focus to it. Task 4 does not use it. No other component changes.

- [ ] **Step 1:** In `settingsLayout.test.tsx`, add tests that the heading gets `id` and `tabIndex -1` with `titleId`, and neither without it. Watch them fail, then add the prop. Whether a landing heading draws a ring stays as Licence's does on `main`; the report says which it is.
- [ ] **Step 2:** About:
  - `PageHeader level={1}` titled About.
  - Sections at level 2: This build (the `dl` kept), Formats (the table as the box's whole content) and Licence (its one sentence in the box).
  - While loading, only the header is drawn, as only the h1 is on `main`.
  - Keep the `section[aria-label=About]` landmark `Routes.test.tsx` pins. Its class names may change; re-pin what reads them.
- [ ] **Step 3:** Licence:
  - `PageHeader level={1}` with `back={{ to: '/about', label: 'About' }}`.
  - The contents `nav` under the header.
  - Each `Licensed` block is a `Section level={2} titleId={id}` holding its terms sentence and `pre.licence-text`.
  - The fragment logic finds the heading by id as before.
  - The heading's `scroll-margin-top` is recomputed for a heading that now sits above its box: a contents link lands the heading below the phone bar at 390, and below the page top at 1280.
- [ ] **Step 4:** Re-pin the About and Licence tests, and the Shell cases, against the sections:
  - Licence's sections are the same width as About's.
  - The back link is still a 44px target under a coarse pointer.
  - The landing case asserts the heading's top is below the phone bar, measured on the heading.
  - Extend `fieldNames.browser.test.tsx`'s outline to About and Licence: h1, then each section's h2.
- [ ] **Step 5:**
  - Hand-apply two defects and watch the tests fail: drop `titleId` from one licence (the contents link must fail to land), and a scroll margin of 0 (the heading must land under the phone bar at 390).
  - Delete the dead `.about-card`/`.about-heading` rules (report the greps).
  - Run node, jsdom and browser whole, then typecheck and lint.
  - Report the counts per file before and after. Commit.

### Task 2: `/account` on the components; the panel keeps its look (spec §1, §4)

**Files:**
- Modify: `web/src/Account.tsx` (465 lines):
  - `Look` (`:35-58`) and `LOOKS` (`:75-108`).
  - The shared `sections` fragment (`:265-429`) and `SectionHeading` (`:261`).
  - The page wrapper (`:431-437`).
- Modify: `web/src/styles/admin.css`:
  - `:1970-2016` (`.account-card`, `.account-heading`) and `:2200-2271` (`.account form .field`, `.account-row`, `.account-wide`, `.account .secret`).
  - `.account .field` (`:563-592`) and `.account .account-row input` in the control lists (`:635-700`).
  - The coarse-pointer `.account-wide` (`:2486`).
  - The page-only rules this task leaves dead.
- Modify tests:
  - `Account.test.tsx` (~37): the page's classes (`:196-304`), its shape (`:94`), the outline (`:1194-1215`), `fields(...)` (`:1221-1243`) and the order (`:1288-1330`).
  - `Account.browser.test.tsx`'s page cases (`:337+`).
  - `admin-account.css.test.ts` (~7).
  - `markup.browser.test.tsx` (`.account > h1`, `:203-230`).
  - `fieldNames.browser.test.tsx`: the account boxes (`:506-523`) and the outline.
  - `classNames.test.ts`: `NAMED['./Account.tsx']` and any `HOOKS` exemption that changes.
- Read first:
  - `Account.tsx` whole.
  - `AccountPage.tsx:582-980`: PR 2's Row with a select, a checkbox and a password, report-only rows with Revoke, and `HeldButton`.
  - `testing/settingsRows.ts` (`expectRowsWired`, `section`, `row`).

**Interfaces:**
- Consumes: `PageHeader`, `Section`, `Row` and `expectRowsWired` as they stand.
- Produces: the page placement's sections are `Section level={2}` titled Password, API keys and Signed in. Task 3 adds behaviour to the same buttons and does not move them.

- [ ] **Step 1:** Split the markup by placement at the section level:
  - The page draws `PageHeader level={1}`, `Section`s and `Row`s.
  - The panel draws its current `reader-*` markup through `LOOKS`.
  - State, handlers and the order of what each section holds stay shared.
  - The page's entry in `Look`/`LOOKS` loses the classes the components now draw. Keep the `section[aria-label=Account]` landmark.
- [ ] **Step 2:** The page's sections, per the design calls:
  - Password: rows Current password and New password, then Change password.
  - API keys: the sentence as description; the address; the row New key with its placeholder, then Create; the secret box; each key as a report-only row with Revoke.
  - Signed in: each session as a report-only row with Revoke; Sign out at the box's foot.
  - The failure alert and the polite note stay above the sections.
  - Report the help-fact table.
- [ ] **Step 3:** Re-pin:
  - The page's class and shape tests, against the sections.
  - The outline: h1 Account, then h2 Password, API keys, Signed in.
  - `fields()` now reads three labels: Current password, New password, New key.
  - The order test: the sentence, then the address, then the form.
  - O175's account boxes: names in the labels' own case. The key box is now named by its label, not its placeholder.
  - Add one test that calls `expectRowsWired` on the page.
  - The panel's tests pass with no edit (Global Constraints).
- [ ] **Step 4:** Watch each defect-named test fail against its defect. Delete the dead page rules (report the greps). Run node, jsdom and browser whole, then typecheck and lint. Report the counts per file. Commit.

### Task 3: A made key is announced, and `/account`'s submits keep focus (O228, Rulings 1–3)

**Files:**
- Modify: `web/src/Account.tsx`: `create` (`:217-234`), `change` (`:236-257`), `freshSecret` (`:146`), the note's polite region (`:276-278`), and the two submits, Change password and Create.
- Modify tests: `Account.test.tsx`, `Account.browser.test.tsx`, `App.test.tsx` if the panel's dialog case reads the buttons.
- Modify: `docs/status.md`: close O228 with the five-part edit (the table row, its prose bullet, the closed count and "leaving N" sentence, the "What remains" line). The web test counts are re-measured in Task 6, not here.

- [ ] **Step 1:** Write the failing tests, in both placements:
  - After Create answers, the polite region's text is `Key made: <name>. Copy it now — it is not stored and cannot be shown again.`
  - The region's text never contains the secret. Assert on the region's element, not the page.
  - Focus stays on Create.
  - If the server lets a key be made with no name, the line is `Key made. Copy it now — it is not stored and cannot be shown again.`, and it has its own test.
  - The note's lifetime is the one "Password changed" has. Read it and test the same clearing.
- [ ] **Step 2:** Implement the announcement through the existing note region. No new live region.
- [ ] **Step 3:** Hold the two submits. Both become `HeldButton type="submit" held={busyDrawn}`, with `busyDrawn = useDrawnWait(busy)`. `busy` stays shared: one request out holds both buttons. The handlers' `if (busy) return` guards hold presses from the send. No wait words are added; neither button draws any on `main`. Tests, in a real browser, for each button:
  - A click keeps focus on that button through the wait.
  - The held look appears only after 200 ms.
  - A second press during the wait sends nothing.
  - Watch each fail against `disabled={busy}` restored.
- [ ] **Step 4:** Close O228 in `docs/status.md`. Run node, jsdom and browser whole, then typecheck and lint. Report the counts per file, and name each panel assertion touched and why. Commit.

### Task 4: Rules on the components, at its own width (spec §1, §4)

**Files:**
- Modify: `web/src/RuleEditor.tsx` (857 lines; the page `:616-856`: the h2 `:618`, the alerts `:619-634`, `div.editor-panes` `:637`, `div.editor-right` `:690`, PreviewTabs `:751`, `details.save-derive` `:793`, `div.editor-actions` `:799`). `RuleList.tsx`, `RuleForm.tsx` and `PreviewTabs.tsx` only where the boxes need it.
- Modify: `web/src/styles/admin.css`: `.rule-editor` and `> h2` (`:912-921`), `.editor-panes` (`:923-951`), `.editor-right`/`.editor-actions` (`:953-962`), and the panes' own borders or backgrounds, if any, that a box now draws.
- Modify tests:
  - `RuleEditor.test.tsx` (~60). The preview's parent has class `rule-editor` and is not in `.editor-right` (`:173-182`); keep what that pins, the preview spanning both panes.
  - `RuleEditor.browser.test.tsx` (`.rule-editor > h2`'s font, `:124`).
  - `classNames.test.ts`'s Rules `HOOKS` exemptions (`:179-202`), if classes change.
  - `SideBySide.scrollbars.browser.test.tsx`'s `/config/rules` case (`:257-286`) must still pass.
  - `fieldNames.browser.test.tsx`'s outline.
- Read first: `RuleEditor.tsx:600-857`, `ScanReport.tsx:220-245` (`ReportLevel`) and `Config.tsx`.

- [ ] **Step 1:** Build the page from the components, per the design calls:
  - `PageHeader` titled Rules, with the four alerts under it.
  - The two panes, each in an untitled `Section`, inside the existing `editor-panes` grid.
  - The preview in a `Section` titled Preview, below and spanning both panes.
  - Then the save-derive disclosure and the action row, as on `main`.
  - DeriveSection's level follows its place in the outline: h1 hidden, h2 Rules, h3 Preview.
- [ ] **Step 2:** Tests:
  - The outline: Configuration's h1, h2 Rules, h3 Preview. Extend `fieldNames.browser.test.tsx`'s outline to Rules.
  - Rules keeps its width: in a real browser at 1280 and 1920, the header and the preview's section are as wide as the panes' grid, and wider than 40rem. Watch it fail with `--config-measure: 40rem` set on `.config`.
  - The preview still spans both panes.
- [ ] **Step 3:** Re-pin what reads the old structure. Watch each defect-named test fail against its defect. Delete the dead rules (report the greps). Run node, jsdom and browser whole, then typecheck and lint. Report the counts per file. Commit.

### Task 5: Rules' waits drawn at 200 ms, and its held controls keep focus (Rulings 2–3)

**Files:**
- Modify:
  - `web/src/RuleEditor.tsx`: Save (`:807-816`, `disabled={frozen||busy||preview===undefined}`, "Saving…" at once), Import (`:819-825`, `disabled={frozen}`, "Importing…" at once), and "Previewing…".
  - `web/src/PreviewTabs.tsx`: `.reorder` `:474` and `.edit-winner` `:483`, `busy||frozen`; "Previewing…" `p.busy` `:210`.
  - `web/src/RuleList.tsx`: the `frozen` sites `:179`, `:187`, `:209`, `:229`, `:267`. `draggable={!frozen}` stays.
- Modify tests: `RuleEditor.test.tsx` (32 `.disabled` reads), `RuleList.test.tsx` (6), `PreviewTabs.test.tsx` (2), and the browser files for the focus cases.
- Read first: `RuleEditor.tsx:80-130` (`frozen`, `busy`, and O96's never-disabled form), `HeldButton.tsx`, `hooks.ts:100-160`, and `Backup.tsx:70-150` (a held, waiting button with `WaitWords`).

- [ ] **Step 1:** List every control Save, Import or a preview holds. For each, say whether it can have focus when the hold starts and how, and whether `main` drops that focus (check it in a real browser on `main`). The report carries this table. A control that cannot have focus at that moment is still held the same way, so the rule has no exceptions to remember.
- [ ] **Step 2:** Write the failing browser tests. Save, Import (after the file pick), Reorder and Edit winner each keep focus through their wait, and a second press sends nothing.
- [ ] **Step 3:** Hold each control with `HeldButton`, or `aria-disabled` plus a turned-down change for the list's checkbox. The handlers' guards hold presses from the send. "Saving…", "Importing…" and "Previewing…" are drawn only once their wait has lasted 200 ms, and Save and Import keep the wider word's box (`WaitWords`, `.keeps-wait-width`). RuleForm stays never-disabled (O96).
- [ ] **Step 4:** Re-pin each `.disabled` read to what it now asserts: held (`aria-disabled`), and a press turned down. Save waiting for a preview is held too, since a save's re-preview leaves Save focused while `preview` is `undefined`. Watch each defect-named test fail against its defect. Run node, jsdom and browser whole, then typecheck and lint. Report the counts per file. Commit.

### Checkpoint: the user tries it

The controller serves the branch's built app per the live-server memory (`127.0.0.1:4399`, a scratch copy of the test library) and asks the user to look at:

- Rules: the untitled boxes for the panes, the Preview box, and the action row left at the foot;
- `/account`'s rows (and the reader's account panel, which should look as it did), with Create's announcement;
- About and Licence, including a contents link on a phone width;
- the waits on Rules and `/account`.

Their rulings are recorded before Task 6. A ruling that changes a task's code goes back to that task's implementer by SendMessage, or to a new task here.

### Checkpoint rulings (the user, 2026-10-25)

1. **"Key made" moves nothing.** (Task 3, round 2.)
2. **Revoking the shown key removes its box.** (Task 3, round 2.)
3. **The shown key's box copies the key on a press.** (Task 3, round 2.)
4. **Licence: each licence section gets a "back to top" link.** (Task 1, round 2.)
5. **A Confirm password box on all three places a new password is typed**: Change password on `/account` (and so in the reader's account panel), an admin's Set password on an account's page, and New account. (Task 5c.)
6. **O229 is filed**: the reader's account panel opens only by Shift+A, so a phone cannot open it from a book. It is not fixed here.

Admins can already set another account's password on its page. The user asked; nothing changes.

**Superseded or added since.** The user's later rulings, each as built in spec §4:

- **R7** (2026-10-25, re-try): a refused Create is said at New key, moving nothing; the callout it is said in, and Change password's refusals said the same way, were my calls. §4, `/account` and Callouts.
- **R8**: a create and a revoke each move the page once; the key's box and its row come and go together. §4, `/account`.
- **(a)** The line saying the passwords differ shows wherever focus is while they differ, not only while focus is in a password box. §4, Confirm password.
- **(b)** "Password changed" moves nothing: a hidden polite region by the form, the Signed in list's change in the same render. §4, `/account`.
- **(c)** The panel's confirm box is labelled Confirm; the page keeps Confirm password. §4, `/account`.
- **Back to top on each licence heading's line**, where the user asked for it, and still at its box's foot ("or it can be in both locations": both was my call). Adds to ruling 4. §4, Licence.
- **About's two licence links**, each to its own section, a row each (the user offered two links or the whole sentence; two was my call). §4, About.
- **U1** (2026-10-26): a callout lies inside its own section, covering what it lies over whole, and nothing moves. §4, Callouts.
- **U2**: the shown key's box draws "Copied" on itself, with a hover look. Adds to ruling 3. §4, `/account`.
- **U3**: a press of a submit held by the mismatch says the line, for an empty or a prefix confirm too. §4, Confirm password and the held controls.
- **U4**: the caption names its key; Rules' "Previewing…" moves nothing; Rules' foot row shares a baseline; `/account`'s Copy address ends its row. §4, Rules and `/account`.
- **U5**: a callout keeps the box's padding from its edges. §4, Callouts.
- **U6**: the line's timing stays: quiet while the confirm is the start of the password. §4, Confirm password.
- **U7**: a refused Create lies right under New key, over the next row in API keys. §4, Callouts.
- **U8**: on New account, Cancel stays in view under the line. §4, Callouts.
- **U9**: the small scroll that shows a refused Create whole where it would not fit stays. §4, Callouts.
- **U10**: New account's line covers Role's select alone. §4, Callouts.
- **U11** (2026-10-26, re-try): a press on the line saying the passwords differ reaches nothing beneath it, as a press on a refusal does; Tab still reaches what it covers. Replaces (a)'s pass-through. §4, Confirm password.
- **U12**: Rules stops short of the right edge as it starts short of the left, at a measure of its own on a wide screen. §4, Rules.
- **U13**: the sidebar's wordmark is a link to `/`, named "taiyaku, Libraries"; Login's stays plain. §4, the sidebar's wordmark.
- **U14** (2026-10-26, after the re-try's fix round): Rules starts where the other Configuration pages do, the room past its measure on its right. Replaces U12's centring. §4, Rules.
- **U15**: the row under Confirm password keeps the room the line saying the passwords differ takes there, said or not, so the line lies over no box at any width and moves nothing. §4, Confirm password.
- **U16** (2026-10-26, looking at Rules at about 1815 px): Rules' boxes are each as wide as what they hold, from the left: the list as wide as its widest rule up to a cap, its names never broken inside a word, the rule's pane as wide as its sentence or its form, and the preview at least as wide as the two together (the user's pick), wider only where a tab needs it, up to `main`'s width; Export beside Import, Import's help under the three. Replaces U12's and U14's measure; U14's start stays. §4, Rules.
- **U17** (2026-10-26, looking at Rules with a rule picked): the four boxes under Sets are as wide as what they hold, about 12 characters, so a picked Rules is narrower than the fixed layout's row; where the list and a picked rule's form do not both fit, the list wraps its names first and the form keeps its own width, so the list reflows when a rule is picked. Amends U16. §4, Rules.

### Task 5c: Confirm password (checkpoint ruling 5)

**Files:**
- Modify: `web/src/Account.tsx` (the password form, both placements), `web/src/AccountPage.tsx` (the Password section, Set password), `web/src/NewAccount.tsx`.
- Modify tests: `Account.test.tsx`, `Account.browser.test.tsx`, `AccountPage.test.tsx`, `NewAccount.test.tsx` (or wherever New account is tested), and `fieldNames.browser.test.tsx` (O175's names for the new boxes).
- Read first:
  - `NewAccount.tsx:75-230`: the name clash. It holds Create account at once while the name clashes, says why through `NameClash`, and moves focus that the hold takes to the name box with `useFocusLeavesHeld`.
  - `AccountPage.tsx:440-470` and `:860-920`: Set password and its `nothingToSet` hold.
  - `Account.tsx`'s password form.

- [ ] **Step 1:** In each of the three forms, add a box "Confirm password" under the new-password box, with the same `type`, `autocomplete` and row or field markup as that form's new-password box. The panel uses the panel's markup.
- [ ] **Step 2:** The button that sends the new password (Change password, Set password, Create account) is held while the two boxes differ.
  - The hold works as the name clash's does in New account: the same mechanism, the same look, and focus the hold takes goes to the confirm box.
  - A line saying the two passwords do not match is drawn once the confirm box is not empty and is not the start of the new password. So it shows for a typo, or for a confirm longer than the password, and not while the confirm is still being typed.
  - The line is said where that form says a clash or refusal today, and moves nothing that line does not already move. The confirm box's `aria-describedby` names it while it is drawn.
  - A send that works empties both boxes.
- [ ] **Step 3:** The server takes one password, as today. No API change. Enter in either box submits only when the two match.
- [ ] **Step 4:** Tests in each form, each watched failing first:
  - held while they differ, and free when they match;
  - the line drawn for a typo, and not for a prefix;
  - the line named by the confirm box's `aria-describedby`;
  - a press and Enter while held send nothing;
  - focus goes to the confirm box when the hold takes it;
  - both boxes emptied after a send that works;
  - O175: the new box's name is its label's words.
  - In the panel, the new field's look matches its other fields.
- [ ] **Step 5:** Run node, jsdom and browser whole, then typecheck and lint. Report the counts per file. Commit.

### Task 6: Docs, the outline, and the gate (spec §6)

**Files:**
- Modify: `docs/status.md`:
  - The Later entry "group the Configuration screens" (`:5541-5587`) says PR 3 shipped and what it holds.
  - The `web/` row of What is built (`:363`) names the screens on the components.
  - Re-measure the web test counts from the node and jsdom runs; do not add to them.
- Modify: `docs/status.md`'s About and Licence entry (`:5612-5629`), where it describes cards.
- Modify: `README.md:283-296`. It says the preview is on the right of the rule editor, but the code draws it below both panes. Make it say what the page draws.
- Modify: `web/src/styles/admin.css` and `settings.css`. A last sweep for rules the four screens left dead, with each selector's grep.

- [ ] **Step 1:** Docs as above.
- [ ] **Step 2:** Confirm `fieldNames.browser.test.tsx`'s outline covers every page in PRs 2 and 3 (Tasks 1, 2 and 4 added theirs).
- [ ] **Step 3:** Run `scripts/check-all.sh` into a log. Quote its `all green` line and its exit status. Report any coverage floor that moves.
- [ ] **Step 4:** Commit.

### Task 7: The look-over (report only)

- [ ] By eye, on the built app (production CSS differs from dev, O218). Cover both themes at 1920, 1366, 1024 and 390, on Rules (a rule selected, and none), `/account`, the reader's account panel, About, and Licence (after a contents link). Put screenshots in the scratchpad, say what each one shows, and change nothing. The controller triages what it finds.

### After the tasks

The whole-branch review (`scripts/review-package a4c58019 HEAD`, split by slice if the diff is large), one fix wave, a review of the fixes, the claim check over the status entries, README and PR body, then the push and PR asks. No Rust is planned; if none changed, the mutation sweep is recorded as not run, with `git diff --stat main -- crates` as the reason.

# The accessibility pass — design

**Date:** 2026-09-26. **Branch:** `a11y-audit`, from `main` at `a173dd3c`.
**Owns:** O135, O154, O156, and whatever the audit below files.

The pass the style pass spec (2026-09-17 §5) put after ⑤: every screen the
style pass restyled, checked with VoiceOver on the phone and a screen reader on
the desktop, "rather than each PR checking its own screens alone". The user
decided on 2026-09-18, when ②'s device pass reached VoiceOver: "keep the
accessibility stuff as a pass at the end".

**Three parts, decided by the user on 2026-09-26**:

1. **An audit that changes no code** (§1), landed as a docs-only PR so both
   fix PRs start from a `main` that carries its findings.
2. **The reader's keyboard and screen-reader access** (§3, O135), designed
   once the audit has answered the questions §1.4 lists.
3. **The markup** (§4: O154, O156, and what the audit files against the
   other screens).

The reason for the order: O154 and O156 are decisions, and what a screen
reader actually announces is the evidence that decides them; and O135's
redesign has to answer VoiceOver as well as the keyboard (§1.4), which only
the phone can measure.

---

## 1. The audit

### 1.1 What it covers

Every screen the style pass restyled. The labeller is out, as it was out of
the style pass (2026-09-17 §6).

| from | screens |
|---|---|
| ① | login |
| ② | the library home and its continue strip, the grid and the table, the series page, search, the drawer below 900 px and the sidebar at 900 px and up |
| ③ | the reader: the top bar, the bottom bar (strip, progress, readout), Settings, Help, the note list and the note editor, Credits, the correction banner |
| ⑤a | rules, and its preview tabs |
| ⑤b | roots, users, backup, the scan report |
| ⑤c | the proposals inbox and a proposal's detail, the account screen as a page and as the reader's overlay |

④ is touch and adds no screen. Whether VoiceOver's double-tap reaches the
page through ④'s handlers at all is not known, and is §1.4's first question.

### 1.2 Setup

- A copy of the real library (`/home/davey/temp/manga-test/library.db`) in
  `target/`, served by `taiyaku serve --db <copy>`, with vite on
  `127.0.0.1` in front of it and `tailscale serve` for the phone
  (tailnet only; turned off afterwards). The original's checksum is recorded
  before and after.
- A second account, `reader`, on the copy, so O159's checkbox has a card
  other than the administrator's to be ticked on.
- Where the copy has nothing to show on a screen — no open proposals, no
  notes, no credit pages — a scratch library that does, named in the notes
  beside the screen it served.

### 1.3 The desktop half

Chromium, driven by Playwright, at 1440×900 and 390×844. **Not a screen
reader**: this reads what Chromium hands one (role, accessible name, state),
not how any one of them phrases it. The user chose it for now, with Orca on
this machine to follow later (2026-09-26). For every screen in §1.1:

1. **The accessibility tree**, as Playwright's ARIA snapshot: every
   control's role, name and state.
2. **A keyboard walk with real key presses**: `Tab` from the top of the
   document to where focus leaves it, recording each focused element, whether
   `:focus-visible` draws anything, and every point where focus lands on
   something not drawn or leaves the document early.
3. **Every dialog and drawer**: does focus move in when it opens, does
   `Escape` close it, and where does focus land when it closes.

Findings from this half shape the phone half's checklist.

### 1.4 The phone half

VoiceOver on the user's iPhone over the tailnet, as a numbered checklist of
about fifteen items plus a swipe through each screen. The items already
known:

1. **The reader's top bar.** With the chrome hidden, `Chrome.tsx` returns
   `null`, so the bars are not in the document for VoiceOver to find. **With
   VoiceOver on, does a double-tap on the page bring the bars up**, and can a
   swipe then reach ⚙ and the layout buttons? This is the question §3 turns
   on (see §1.5).
2. **Settings** opened from ⚙: does VoiceOver move into it, and does the
   escape gesture (a two-finger Z) close it?
3. **The drawer**, below 900 px: its Close button, and whether the escape
   gesture closes it — named in the style pass spec's own paragraph.
4. **The one-of-N rows** (O156): what VoiceOver says for the chosen button
   in a row using `aria-pressed` against one using `aria-current`.
5. **The permission checkboxes** (O154): what VoiceOver says entering the
   bare `div` group against the `fieldset` beside it.
6. **#176's device pass, which the user merged without**: tick "May stream
   pages" on the `reader` card at `/config/users` and see the other boxes keep
   their colour (O159); tap just above and below the words of the config nav,
   the inbox's kind tabs and the preview tabs, and beside a rule's enable box
   (O157); no zoom, no sideways scroll.

### 1.5 What the phone decides for §3

O135's redesign has three candidate directions. Each depends on something
only the phone can answer, which is why §3 waits for it:

- **`Tab` shows the chrome and moves focus into the top bar**, and `Escape`
  hides it and hands focus back. A keyboard answer: it serves VoiceOver only
  if a double-tap on the page already brings the bars up (§1.4 item 1).
- **An always-present "Show controls" button**, visually hidden, the first
  thing a screen reader reaches. Its worth depends on whether VoiceOver can
  reach the chrome today.
- **The chrome kept mounted, `inert` while hidden.** `Chrome.tsx`'s doc
  argues against it: the unmount is what keeps a reader who never opens the
  chrome from fetching a single filmstrip thumbnail, and `loading="lazy"`
  was measured not to do that job. Last, unless the audit shows the unmount
  blocking something the other two cannot reach.

### 1.6 What the audit leaves

- A section in `docs/notes.md` with the desktop measurements and the user's
  phone answers, quoted rather than paraphrased where a row will cite them.
- An O-row for every finding not already filed.
- **O156's row corrected in place.** It says four one-of-N button rows mark
  the chosen one two ways, and the count is short: the reader's layout
  buttons (`TopBar.tsx`) and the note panel's colour swatches
  (`NotePanel.tsx`) use `aria-pressed` too, `RuleList.tsx` puts
  `aria-current` on a selected `<li>`, and `Filmstrip.tsx` puts it on the
  current tile's `<button>`. The row keeps its original count and says where
  it was wrong.
- §2 below, filled.

It fixes nothing. A defect found here is filed, not fixed, however small.

---

## 2. What the audit found

The desktop half ran on 2026-09-26 and the phone half on 2026-09-30; the
measurements and the user's answers, quoted, are in `docs/notes.md`, "The
accessibility audit". Five rows opened and three widened:

| row | what | for |
|---|---|---|
| O135, widened | `Shift+Tab` reaches ⚙ and no other top-bar control; on the iPhone, a VoiceOver double-tap on the page brings the bars up | §3 |
| **O161** | only the note editor takes focus on opening, no panel hands it back on closing, and Credits stays open on `Escape` | §3 |
| **O162** | with VoiceOver on, the user could not turn a page from the page; whether the strip can is not established | §3 |
| O154, widened | VoiceOver read the bare three and the `fieldset` alike | §4 |
| O156, corrected | the count was short (`TopBar.tsx`, `NotePanel.tsx`, `RuleList.tsx`, `Filmstrip.tsx`); `aria-pressed` is spoken as "toggle button, pressed" | §4 |
| **O163** | an `h1` on two of ten screens; the reader has no heading or landmark | §4 |
| **O164** | the Proposals link is named "Proposals17" in Chromium, and VoiceOver said no count | §4 |
| **O165** | the Exclude globs field's name carries its whole hint | §4 |

**Clean**: on the ten screens walked, at both widths, every stop walked
draws a ring and none is off screen or covered (home and series were stopped
at 80 presses); the drawer takes focus, closes on `Escape` and hands focus
back to ☰. On the phone, O159 held (the other boxes kept their colour
through a save), O157's rule-enable box toggles on a tap beside it, with no
zoom and no sideways scroll.

**Not answered**: the escape gesture, whose one answer does not say which menu
it closed; the rotor, which the user had not met before; whether a strip tile
turns a page under VoiceOver; whether "Close menu"
closes the drawer under VoiceOver; whether the readout announces the page a
turn lands on; O157's underlined rows, where the first answer was "I'm not
sure exactly where I should be pressing" and the retry followed an instruction
to tap 1 cm from the word, several times further than the 12 px the target
reaches, so it says nothing either way; and any desktop screen reader, which
is Orca, later. **Not covered, though §1.1 and §1.2 promised them**: the scan
report, the table layout and a proposal's detail were not captured, and
Credits and All notes were audited empty.

**What §1.5 asked, answered.** A VoiceOver double-tap on the page does bring
the bars up, so the first candidate reaches VoiceOver readers as well as the
keyboard, and the second, a hidden "Show controls" button, is not needed to
reach the chrome. Whether a VoiceOver reader can turn a page at all, from the strip
or otherwise, is still open (O162), and so is what the escape gesture does;
§3's device pass has to try both. §3 starts there.

## 3. The reader (O135, O161, O162)

**Designed 2026-09-30. Branch:** `a11y-reader`, from `main` at `77819d3e`.
**Owns:** O135, O161, O162, and O166, which this design files if the
branch's first task reproduces it (§3.2).

§1.5's first candidate, as §2 left it: `Tab` shows the chrome and moves focus
into it. Three choices the user made on 2026-09-30 shape the rest:

- **A ring, not a trap.** `Tab` walks the bars' controls and comes back to
  the page, which hides them — offered against a trap that wraps inside the
  bars, and against `Tab` still toggling with another key moving focus in.
- **The arrows turn pages in the bars.** Offered against the arrows
  belonging to the bars, as they do today.
- **Hidden turn buttons for VoiceOver.** Offered against visible `‹` `›`
  buttons in the bottom bar and against the page as a native range control
  (VoiceOver's swipe up or down).

### 3.1 The ring (O135)

The reader's keyboard stops, in order:

1. the page;
2. the top bar: Library, `single`, `two`, `scroll`, ⚙;
3. the strip, one stop (§3.2);
4. whatever the page itself offers, when it offers anything: a gap's moves
   (`GapOffers`), the note tabs (`Annotations`), the note list (`NoteList`);
5. the page again.

*Since 2026-11-21 the top bar's stops are the way back, the series link where
the work is in a series (O427), the layouts and ⚙, and the strip's stop is
followed by Help while the help module is on and a mouse or a trackpad is
there (O425).*

**The bars are up while focus is on one of their stops**, and focus leaving
them — for the page or for the page's own controls — hides them. `Shift+Tab`
walks the same ring backwards. The
bars go down *before* focus reaches a note-list row, because O112 hides that
list while they are up and a `visibility: hidden` element takes no focus.

**`Escape` anywhere in the bars hides them** and puts focus on the page,
below every panel and a pending correction in precedence (§3.3) — and so does
`Escape` pressed on the page while a tap has the bars up. *Since 2026-11-21
(O419): only while the keyboard holds the bars, its focus in them or a key
having brought them up with no pointer pressed since. Bars a tap or the
mouse brought up take no `Escape`, which then leaves the reader by its way
back (§3.3).*

**`isInteractivePress` has to leave the page area out.** It counts any
`[tabindex]` ancestor as a control, so the page area's own `tabIndex` would
make every press on the page a press on a control and `gestures.ts` would
refuse every tap and swipe. Found while planning, from `keys.ts`; the
gesture tests already in `App.test.tsx` are what fail if the exclusion goes.

**The page is a real focus target**: the page area, `tabIndex={-1}`, drawn
with no ring, since focus there means reading and nothing else. Focus comes
back to it rather than to `<body>`, so a desktop screen reader has something
to land on. `Chrome` still returns `null` while hidden (O143 is untouched), so
`Tab` from the page is claimed — `preventDefault` — and the reader shows the
bars and focuses Library once they have mounted. **Every step of the ring is
the reader's, not only its two ends**: within each region it follows document
order, but the regions' order is the ring's, and the browser's would get it
wrong — `CorrectionBanner` renders after the bottom bar, where the ring puts
it before the strip. *Corrected 2026-09-30: this paragraph first said the
browser's own order was kept within the bars; Task 4's review found the plan
had built the ring owning every step, and that it has to.*

**While correcting**, `CorrectionBanner` has the top edge and the correction
holds the bars up (`beginCorrection`'s `setChromeVisible(true)`). The ring is
the page, the banner's buttons, the strip, the page's own controls, and
leaving the bars does not hide them. `Escape` still cancels the correction.

A tap in the middle third still toggles the bars, and a pointer changes
nothing here. The `Tab` row's `label` in `keys.ts`'s `READER_KEYS` changes to
say what the key now does, and `Help` lists it from there.

### 3.2 Which keys a focused control keeps (O135, O166)

**A reader key stands down only where the focused control uses that key.**
Text entry — an `input` other than a checkbox, a `textarea`, a `select`,
anything `contentEditable` — keeps every key. A button, a link or a checkbox
keeps `Space` and `Enter`. Everything else goes to the reader: `→` turns the
page from ⚙, `s` closes Settings from one of its checkboxes, `c` begins a
correction from the strip. This replaces `isInteractiveTarget` as the
`onControl` guard in `keyPlan`, which stands down every key on a button, a
link, an input and anything carrying a `tabindex` — the attribute the page
area (§3.1) and the panels (§3.3) are about to carry.

The strip becomes **one stop**: the current tile is `tabIndex={0}` and every
other tile `-1`, and a turn moves focus to the new current tile, so the strip
follows the reader as `→` turns. Where no tile is current — a gap, a
borrowed page — the stop is `stripAnchor`'s tile. VoiceOver still swipes to
every tile; `tabindex` does not limit a swipe.

**While correcting, every tile stays a stop, as today.** Picking one is what
the correction is for, and `currentIndex` is `-1` then (`App.tsx` passes
`pending ? -1 : currentIndex`), so there is no current tile to rove to; one
stop would leave the keyboard a strip it could not pick from. Found while
planning.

**O166, filed by this design: `isInteractiveTarget` does not know a
`<select>`.** Four modules put one in Settings (`layout`'s `mode`, `fit`'s
`upscaleFilter`, `zoom`'s `onSwap`, `annotations`' `notesList`), so a key
pressed on one reaches the reader — `s` closing Settings, a digit switching
edition — where it should reach the select. **Found by reading, not
measured.** The branch's first task reproduces it in the browser project with
real key presses, and the row is written from that measurement; a select
that turns out to keep its keys is not filed.

### 3.3 Panels and focus (O161)

**Opening a panel moves focus into it**: to the panel itself, `tabIndex={-1}`,
each already named — Settings, "Keyboard help", "Credit pages in …", "All
notes", "Account" — so a screen reader says the panel rather than the button
that opened it (O161's *"settings, expanded"*). The note editor keeps its
textarea, as `NotePanel`'s focus effect already does. By ⚙ or by a key alike.

**Closing hands focus back** to what held it when the panel opened: ⚙ if ⚙
opened it, the page if a key pressed on the page did, a bar stop if a key
pressed there did. If that element has gone — the bars hidden since — the
page.

**`Tab` inside an open panel stays in it**, walking its controls and wrapping
at either end. It no longer shows the bars behind the panel, which is O161's
first-`Tab` finding: `keyPlan` answers `Tab` with `toggle-chrome` ahead of
every guard but the note editor's and the account panel's.

**`Escape` closes every panel.** `keyPlan`'s `Escape` branch gains the arms
Settings and Credits lack, and the order is: Help, Account, Settings and
Credits, the note panel, a pending correction, the bars. Settings and Credits
go above the note panel for the reason Help already closes first — closing
them discards nothing, where the note panel flushes a note and the
correction abandons one. *Since 2026-11-21 (O419) the order goes on: the
bars only while the keyboard holds them (§3.1), then the reader itself,
which `Escape` leaves by its way back; a text field keeps its own
`Escape`.* `Help.tsx`'s doc says Credits' missing `Escape` has a
reason in `App.tsx`'s listener; there is none — the commit that added Help
(`3c7a4795`) recorded only that neither Settings nor Credits closed — and the
sentence goes.

**Two panels cover the reader and four sit beside the page.** Settings and
Account get `aria-modal="true"`, so VoiceOver stays inside them. Help,
Credits and All notes stay non-modal, and the reader keys keep
working while they are open, as today: focus on the panel itself is not a
control under §3.2. Settings keeps today's behaviour too — a key pressed
outside its fields still reaches the page behind it, so a reader can change
the fit and turn a page to see it. Account keeps standing every key but
`Escape` down (`keyPlan`'s own reason).

### 3.4 VoiceOver turns a page (O162)

**Why it cannot today**: a VoiceOver swipe moves between items and a
double-tap presses the focused item's middle. With the bars hidden the page
image is the only item, and its middle is the middle third, which toggles the
bars — so a double-tap on the page can bring the bars up (§2) and can never
turn one.

**Two buttons, "Previous page" and "Next page"**, mounted whenever the page
is, over the page area's left and right thirds, drawing nothing,
`pointer-events: none` — so a finger or a mouse still reaches the page area
and `gestures.ts` exactly as now — and `tabIndex={-1}`, out of the ring,
because the keyboard has its arrows.

- **They do what `←` and `→` do** in the same state: a physical step, through
  `readerIntent` and so through `stepFor`. In a right-to-left work the left
  third is therefore named "Next page": the name follows the meaning, the
  position follows the page.
- **The DOM order is the page, Next, Previous**, so one swipe right from the
  page reaches Next whichever way the work reads.
- **Not rendered while the reader's keys are stood down**: with the note
  editor open, where a press on the page draws a region, and with Account
  open. Settings is `aria-modal` (§3.3), which already keeps VoiceOver off
  them.
- **Not rendered in scroll mode.** The page area is the scrolling column
  there, so a button positioned inside it scrolls away with the pages, and
  VoiceOver moving onto one would scroll the column back to it. Each group is
  its own image in scroll mode, and a VoiceOver swipe already moves from one
  to the next; the device pass checks that the position follows (§3.7).
  Found while planning.

**The assumption this rests on**: that VoiceOver's double-tap on a `<button>`
presses the element itself, as WebKit's accessibility press does, rather than
tapping the point under it — so `pointer-events: none` does not stop it. The
device pass (§3.7) tests this first. If it fails, the buttons need
`pointer-events` back and a different way to stay out of a pointer's road.

### 3.5 What says where you are (O162)

**One live region**, mounted whenever the page is, visually hidden,
`aria-live="polite"`. After a turn it says where the turn landed, in words:
*"page 6, 33 percent"* — `positionLine`'s page and percentage, without its
abbreviations, since "slot 6 / 18 · p. 6 · 33%" read aloud is noise. The
wording is the device pass's to change. On a borrowed page it says the
authority edition first, as the position line does — *"raw, page 6, 33
percent"* — because the numbers are that edition's. *Added 2026-09-30 after
the whole-branch review found the spoken line dropping it.*

- **Quiet on a gap**, which `Gap`'s own `role="status"` already announces.
- **The readout row loses `role="status"`**. With both, a turn could be
  spoken twice while the bars are up, and the row would announce every change
  to its reading-time readout. The row stays as text VoiceOver can swipe to.

### 3.6 Tests

Every test here is watched failing against the defect it names, with the
defect applied by hand, before it counts.

1. **`keyplan.test.ts`**: §3.2's rule for each kind of control against a
   character, an arrow, `Space` and `Enter`; the new `Escape` order, arm by
   arm; `Tab`'s new plans.
2. **The browser project, with real key presses** (`App.browser.test.tsx`):
   the ring walked forwards and backwards with the bars' state at every stop;
   `Escape` from every stop; `→` on every bar stop turning exactly one page
   with focus following the strip; the ring with a gap, with note tabs and
   during a correction.
3. **Every panel**, opened by its key and, for Settings, by ⚙: focus inside
   after opening, `Tab` staying inside, `Escape` closing it, focus back on
   the opener. A character and an arrow pressed on a Settings select change
   the select and turn nothing (O166).
4. **The turn buttons**: a `click` dispatched on the button element turns one
   page; a real pointer press at the same spot turns exactly one, not two; a
   right-to-left work names them the other way round; neither exists with the
   note editor or Account open, or in scroll mode.
5. **The live region**: its text changes on a turn, and not on a gap; the
   readout row has no `role`.

### 3.7 The device pass

On the user's iPhone with VoiceOver on, over the tailnet, against a
production build behind `vite preview` and a `serve --db` on a copy of the
real library, as §1.2's rig was. Every turn reported is checked against its
progress `PUT` in the server's log before it is written down.

1. From the page, swipe to "Next page" and double-tap: one page turns, and
   the turn is spoken. Again. Then "Previous page".
2. A double-tap on the page still brings the bars up.
3. ⚙ puts VoiceOver inside Settings; Close puts it back on ⚙.
4. The escape gesture on open Settings — two fingers drawing a quick Z,
   shown to the user first. Whether Safari hands it to the page is not known;
   Close is the way out either way.
5. A strip tile, double-tapped, turns a page — O162's open half.
6. In scroll mode, a swipe from one page to the next moves the position the
   readout reports.

And on the user's own desktop browser, by keyboard: the ring, the arrows in
the bars, `Escape`.

### 3.8 What §3 leaves

- **O163**: the reader's heading and landmark are §4's.
- **O143**: `Chrome` still unmounts while hidden; nothing here needs it
  mounted.
- **A desktop screen reader** — Orca, or VoiceOver on the MacBook — later, as
  §2 left it.
- **VoiceOver on the phone**: §3.7's checklist, which the user set aside on
  2026-10-01 for a tester who knows VoiceOver (`docs/notes.md`, "The device
  pass, 2026-10-01"). O162 stays open for it.
- **What the branch filed on the way**: O167 through O172.

§3 closes O135, O161 and O166 as one PR and narrows O162, with the
measurements in `docs/notes.md`. *Corrected 2026-10-01: this first said §3
closes O162 too.*

## 4. The markup (O154, O156, and the audit's rows)

**Designed 2026-10-01. Branch:** `a11y-markup`, from `main` at `16504d61`.
**Owns:** O154, O156, O163, O164, O165, and two rows this design files,
O173 (every screen has the same `document.title`) and O174 (no skip link).
The user folded both into §4 on 2026-10-01 rather than filing them for later.

Four choices the user made on 2026-10-01 shape it:

- **Each one-of-N row takes the role that matches its job** (O156): tabs
  where a row switches panels, toggle buttons in a named group where it
  filters or sets something. Offered against `aria-pressed` everywhere, and
  against radio groups, whose arrow keys would collide with §3's page turns
  on the reader's layout buttons.
- **No visual change for the headings** (O163): a screen that already shows
  its title promotes it; one that shows none gets a visually hidden `h1`.
  Offered against visible titles on three screens, and against each
  configuration tab's `h2` becoming the `h1`.
- **An invisible named group for the permission boxes** (O154). Offered
  against a `<fieldset>` with a visible legend, and against closing the row
  unchanged.
- **Chromium only** for evidence: no device or screen-reader pass (§4.8).

### 4.1 One-of-N rows (O156)

- **`PreviewTabs` becomes tabs.** The `<nav aria-label="Preview">` becomes
  `<div role="tablist" aria-label="Preview">` — it was never navigation,
  since nothing changes the URL. Each button takes `role="tab"`,
  `aria-selected` and `aria-controls`; only the selected tab is in the `Tab`
  order. `←` and `→` move the selection and the focus together, wrapping at
  the ends; `Home` and `End` go to the first and last. Selection follows
  focus, because every panel draws from the preview already held. A key
  pressed with Alt, Ctrl or ⌘ is left to the browser — Alt+← is Back — and a
  key steps from the tab it was pressed on. *Both added 2026-10-01 after the
  whole-branch review, which found the handler claiming modified arrows and
  stepping from the selected tab.* What the
  selected tab draws sits in one `role="tabpanel"`, labelled by that tab.
  *Corrected 2026-10-01 while planning: this first gave the panel
  `tabIndex={0}`, which biome's recommended `noNoninteractiveTabindex`
  refuses on a `tabpanel`; nothing in a panel needs focus to be read.*
- **The library's chips** (All, Multi-edition, Unread) move into a
  `role="group"` named "Show". "Added" stays outside it: it is a sort that
  turns on and off, and `aria-pressed` is right for it. The group is a box of
  its own rather than `display: contents`, whose role some browsers have
  dropped from the accessibility tree; it carries the row's gap, and the row
  is measured on `main` and on the branch at 1440 and 390 (§4.7).
- **The inbox's rows** are named: `.proposal-states` a `group` "State",
  `.proposal-tabs` a `group` "Kind". Both keep `aria-pressed`.
- **`RuleList`**: `aria-current="true"` moves from the `<li>` to its
  `.select` button, which is what takes focus. `admin.css`'s
  `.rule-row[aria-current="true"]` becomes
  `.rule-row:has(> .select[aria-current="true"])`, so the look still follows
  the attribute.
- **`admin.css`'s three-prelude rule** keeps three preludes, with the
  preview's now `.preview-tabs button[aria-selected="true"]`;
  `admin-proposals.css.test.ts`'s selector list moves with it.
- **Unchanged**: the reader's layout buttons and the note colour swatches,
  already toggle buttons in named groups; `Filmstrip`'s current tile, a
  position rather than a choice; `config-nav`, navigation with
  `aria-current="page"`.

### 4.2 Headings and landmarks (O163)

One `h1` per screen, first inside its `main` — first of what the screen
draws, that is: `Shell`'s own failure line, when there is one, comes before
it:

| screen | `h1` | how |
|---|---|---|
| login | the wordmark, *taiyaku* | the `h2` promoted; login's root becomes `<main>`, since it had no landmark |
| home | "Library" | visually hidden |
| search | "Search" | visually hidden |
| series | the series' title; "Series", visually hidden, until it has loaded and when it cannot be shown | the title unchanged. *The hidden stand-in added 2026-10-01 after the whole-branch review: the screen had no `h1` while loading, or at all on a series that is not one* |
| proposals | "Proposals, 17 open" | "Proposals, " visually hidden inside the existing `h1` |
| configuration | "Configuration" | visually hidden, before `config-nav`; each tab's `h2` stays an `h2` beneath it |
| account | "Account" | the page's `h2` promoted; its sections' `h3`s become `h2`s on the page and stay `h3`s in the reader's dialog, so the level follows `placement` |
| the reader | `titleLines`' title and edition line joined by " · ", or the edition line alone before the work's detail arrives | visually hidden and mounted whenever the reader is drawn, so it is there with the bars down; the reader's root becomes `<main>` |

**A promoted heading keeps its size.** `base.css` sizes `h1`, `h2` and `h3`
by element inside `.library` and `.login`, so the login wordmark and the
account page's title would grow a step. Each is held at `--text-h2` — login
in its inline style, the account page by `.account > h1` — and the browser
project compares each one's computed `font-size` with that of an `h2` placed
beside it. The account sections' `h2`s keep theirs from
`.account-heading`, which sets its own.

`.visually-hidden` stays in `reader.css`, which every screen loads through
`index.css`, and its comment says the library's screens use it too.
*Corrected 2026-10-01 while planning: this first moved it to `base.css`,
whose own test (`base.css.test.ts`) refuses a selector not scoped to
`.library` or `.login`.*

**The sidebar's `h2`s** ("Roots" and "Library") stay where they are, before
`main` at 1440: `main`'s `h1` and the landmark both reach past them.

### 4.3 Names (O164, O165)

- **O164.** The link takes `aria-label="Proposals, 17 open"` while there is
  a count — the same words as the inbox's `h1` — and the badge keeps its
  look. With no count it takes no label and is named "Proposals" by its text.
  *Corrected 2026-10-01 while planning: this first put the words in visually
  hidden spans beside the number. A visually hidden span is absolutely
  positioned, which makes it a block, and the browser project's
  `toHaveAccessibleName` then named the link "Proposals , 17 open", a space
  before the comma.*
- **O165.** The field keeps its wrapping `<label>`; its box takes
  `aria-labelledby` pointing at the label's `<span>`, and, where there is a
  hint, `aria-describedby` pointing at the `<small>`. Exclude globs is then
  named "Exclude globs (one gitignore pattern per line)" and described by its
  hint. Ids come from `useId`, since a screen draws one `RootRow` per root.
  Every `TEXT_FIELDS` box takes the `aria-labelledby`, not only the one with a
  hint, so a hint added to another field later cannot bring O165 back. No CSS
  changes.

### 4.4 The permission boxes (O154)

`UserRow`'s `.checks` takes `role="group"` and `aria-label="Permissions"`.
Nothing on screen changes, and no `<fieldset>` is added, so the box model that
once pushed the rule form 26px wide (`admin.css`, above `.rule-form
.rule-sets`) does not come into it. `RootRow`'s `.checks` and the inbox's
`.weak-check` each hold one box and need no group.

### 4.5 Titles (O173)

`useDocumentTitle(name)`, in a new `title.ts`, sets `document.title` to
`"<name> — taiyaku"`, or to `"taiyaku"` while `name` is `undefined` or
empty.
`index.html`'s fallback changes from "taiyaku reader" to "taiyaku".

| screen | title |
|---|---|
| login | Sign in — taiyaku |
| home | Library — taiyaku |
| series | *the series' title* — taiyaku, once loaded |
| search | Search — taiyaku, not changed by typing |
| proposals | Proposals — taiyaku |
| configuration | Rules, Roots, Users or Backup — taiyaku, from the matched tab in `Config.tsx` |
| account | Account — taiyaku |
| the reader | *the reader's `h1`* — taiyaku; a page turn does not change it. "taiyaku" alone until the work has loaded, and for good if it never does. *Added 2026-10-01 after the whole-branch review: the reader draws nothing before its load answers, so its tab kept the last screen's title* |

Every routed screen sets one, so none restores the old title when it
unmounts.

### 4.6 The skip link (O174)

`Shell` draws `<a className="skip-link" href="#content">Skip to
content</a>` as `.library`'s first child. It is visually hidden until it has
focus, then drawn at the top left in the theme's tokens, over the phone bar
and the sidebar.

- **Activating it focuses `main`** (`id="content"`) after `preventDefault`,
  with `preventScroll`: at phone width the bar is sticky and would cover
  `main`'s top edge. **`main` is focusable only while it holds that focus** —
  the handler sets `tabindex="-1"` and takes it off again when focus leaves.
  *Corrected 2026-10-01 after the whole-branch review: this first gave `main`
  a standing `tabIndex={-1}`. A focusable `main` takes focus on every click
  on plain text inside it, and the next `Tab` then starts from its top rather
  than from the click — reproduced in Chromium by the review.* What is left
  of that: while `main` holds the skip link's focus, a click on plain text
  inside it still lands on `main`, until focus moves elsewhere. The link
  carries a `biome-ignore` for `useValidAnchor`, which refuses an anchor with
  both `href` and `onClick`; it stays an anchor because it has a destination. A real `#content` navigation would put `#content` in the
  URL and push a history entry that Back then walks through. The URL does
  not change.
- **`main` draws no ring** when it takes focus: it is where the reader lands,
  not a control.
- **`inert` while the drawer is open**, like the phone bar and `main`, so
  the drawer's trap stays sealed.
- **Not in the reader**, whose ring already starts at the page, **and not on
  login**, which has three stops.

### 4.7 Tests

Every test here is watched failing against the defect it names, with the
defect applied by hand, before it counts. Each asserts per element, by role
and name, not over the whole page.

1. **jsdom**, which here has no name computation (this repository mounts
   without testing-library, by its own rule), so these assert the markup a
   name is computed from: the Proposals link's `aria-label` with a count and
   without; what Exclude globs' `aria-labelledby` and `aria-describedby`
   point at; the "Permissions" group holding exactly the
   three boxes; one `h1` per screen, with its name; the reader's `main` and
   `h1` with the bars down; the preview's tablist, tabs, `aria-selected` and
   the tabpanel's label; each named group's members, with "Added" outside
   "Show"; `aria-current` on the rule's button and not on its `<li>`;
   `document.title` on every route and on login.
2. **The browser project, with real key presses**: `←`, `→`, `Home` and `End`
   on the preview tabs, and `Tab` leaving the tablist from the selected tab
   rather than stepping to the next one; the skip
   link at 1440 and at 390 — the first `Tab` reaches it and it is inside the
   viewport, `Enter` focuses `main` and leaves the URL alone, the next `Tab`
   reaches `main`'s first stop, and with the drawer open it is not reached;
   the chip row's buttons where they were; the promoted headings' computed
   `font-size`; and the computed names and descriptions jsdom cannot give —
   the Proposals link, Exclude globs, the "Permissions" group, the inbox's
   `h1` and the preview's tabs — with `toHaveAccessibleName` and
   `toHaveAccessibleDescription`.
3. **The stylesheet tests**: the three-prelude selector list, the
   `:has()` rule, and the account page's `h1` held at `--text-h2`.

### 4.8 Evidence

The audit's desktop half again (§1.3), on the same ten screens and the
reader, at 1440 and 390: Chromium's accessibility tree, before and after,
against a production build and a `serve --db` on a copy of the real library,
with the original's hash checked afterwards. The measurements go in
`docs/notes.md`.

**No device or screen-reader pass**, by the user's choice. Each row closes on
Chromium's tree and the tests, and says that VoiceOver was not re-measured —
O164 most of all, whose VoiceOver reading dropped the count for a reason
nobody established. What a VoiceOver tester should check for §4 joins the
list waiting for O162's.

### 4.9 What §4 leaves

- **A screen reader on any of it**: VoiceOver, with O162's tester; Orca or
  VoiceOver on the MacBook, later.
- **The sidebar's `h2`s before `main`** (§4.2).
- **The Overrides tab's badge**, named with its count after a space —
  a separated count, unlike O164's.
- **What the branch filed on the way**: O175, Chromium's own tree naming
  `.field` labels in capitals; O176, the reader drawing nothing when its
  work fails to load; and O177, a browser test that
  failed on the fallback font and cannot see the defect it names — its case
  now waits for the fonts. All three older than this branch.

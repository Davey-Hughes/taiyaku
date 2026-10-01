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

**The bars are up while focus is on one of their stops**, and focus leaving
them — for the page or for the page's own controls — hides them. `Shift+Tab`
walks the same ring backwards. The
bars go down *before* focus reaches a note-list row, because O112 hides that
list while they are up and a `visibility: hidden` element takes no focus.

**`Escape` anywhere in the bars hides them** and puts focus on the page,
below every panel and a pending correction in precedence (§3.3) — and so does
`Escape` pressed on the page while a tap has the bars up.

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
correction abandons one. `Help.tsx`'s doc says Credits' missing `Escape` has a
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

*Designed after §2.*

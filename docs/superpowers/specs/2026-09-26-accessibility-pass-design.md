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

## 3. The reader (O135)

*Designed after §2, from §1.5's candidates.*

## 4. The markup (O154, O156, and the audit's rows)

*Designed after §2.*

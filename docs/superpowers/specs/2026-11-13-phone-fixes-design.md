# Phone fixes: O229, O171, O141, O172, O274

A short design for one web PR. The survey behind it, read at 635b91bb, is
`.superpowers/sdd/phone-family-evidence/survey.md` (not committed).

## Intent

Family members are about to get accounts on the NAS server, and they will
read on phones. Each of these five open rows is something a phone reader
meets early: an account panel they cannot open from a book, a zoom they did
not ask for, a line that runs past its edges, two numbers for one page, and
a page that scrolls sideways.

## Goals

1. **O229.** From inside a book, a tap reaches the account panel (password,
   API keys, signed-in sessions, Sign out).
2. **O171.** Focusing the note editor's text box on a phone does not zoom
   the page.
3. **O141.** The readout line under the page stays inside its row on a
   phone at 390 px, ending in "…" when it does not fit.
4. **O172.** The note editor numbers a page the way the readout does,
   counting from 1, everywhere it prints one.
5. **O274, its 390 px half.** A long archive path in a stamp plan wraps, so
   the Series page does not scroll sideways on a phone.

## Non-goals

- O274's 1280 px half: the open volume's panel stays in its tile. The user
  settled on 2026-09-18 that the alignment panel and metadata editor fit
  inside their tile (`2026-09-17-style-pass-design.md:457-466`). O274 is
  narrowed to that half, not closed.
  *Reopened 2026-11-14 by the owner: the open volume's panel lies over the
  rows below, two tiles and the gap wide, and O274 is closed; see
  `2026-11-14-compact-series-tiles-design.md`.*
- The other phone rows (O139, O142, O162, O167 and the rest).
- Any change to what the readout says or the order it says it in.

## Anti-goals

- **The reader's top bar does not change.** Its make-up (Library, title,
  layout, ⚙) was settled with the user on 2026-09-19 (style-pass §8.3,
  `:768-777`), and a new stop would narrow the phone title further.
- **The readout row stays one line** (style-pass §8.4,
  `:833`: it keeps `readoutRow`'s text, order and ellipsis). Wrapping to
  two lines would fix the overflow by breaking that ruling.
- **No two dialogs open at once.** Settings and the account panel are each
  a modal; one replaces the other.

## Approaches (O229)

The row named three. **The user picked the Settings entry** on 2026-11-13.

- **An Account entry in the ⚙ Settings panel** (picked). The bar is
  untouched, and the panel opens over the book, so a reader stays in it.
  Costs one extra tap.
- A stop on the reader's bar: one tap from anywhere, but it changes the
  bar settled in the style pass and leaves a phone title of about 36 px
  (worked out from the CSS, not measured).
- A link to `/account`: simplest to build, but it leaves the book.

The other four rows each have one fix that the row and the code imply;
there was no choice to make.

## Design

- **O229.** The Settings panel gets an "Account…" button. Pressing it
  closes Settings and opens the reader's account panel, the one Shift+A
  opens today. Shift+A stays, and pressed while Settings is open it does
  what Account… does (the owner's ruling of 2026-11-13), so two dialogs are
  never up at once. When the account panel closes, focus goes back where
  closing Settings would have put it: to ⚙ when ⚙ opened Settings, to the
  page when a key pressed on the page did, as the accessibility design's
  §3.3 (`2026-09-26-accessibility-pass-design.md:287-291`) has every panel
  do. The Settings entry that opened the account panel has gone with
  Settings, so it cannot be the target. *My call:* the button sits in the
  panel's head row beside Close, not at the foot of the panel. The panel
  shows twelve modules' settings, and at the foot a phone reader would
  have to scroll past all of them to find it. The entry is not a module, so Settings' rule that
  "a new module needs no edit to this file" still holds.
- **O171.** The note editor's text box drops its inline `font: 'inherit'`
  (`NotePanel.tsx:183`), and a new coarse-pointer rule gives the docked
  panel's text fields the reader's 16 px floor. *My call:* a separate rule,
  not a wider selector on the dialog floor, because
  `reader.css.test.ts:104-121` matches that rule's selector text exactly.
- **O141.** The "…" truncation moves from the flex row
  (`Chrome.tsx:159-169`), where it has no effect, onto a non-flex child
  that holds the text, so the text stops at the row's padding.
- **O172.** The three places `NotePanel.tsx` prints a slot (`:165`,
  `:338`, `:343`) print it counting from 1, as `modules/readouts.ts`'s
  `positionLine` does. The tests that pin the 0-based print change with
  them.
- **O274.** The stamp plan's paths (`MetadataEditor.tsx:606`) get the
  break points `PathWithBreaks` gives paths elsewhere, plus
  `overflow-wrap: anywhere`. `path` and `archives` come out of
  `classNames.test.ts`'s list of classes no style reads.

*Amended 2026-11-13, after Task 5, by the controller's rulings:* O274's
paths alone did not meet goal 5. At 390 px the editor's title box at its
default width, the title line, the consequences'
`<content_key>.library.json`, a stamped content_key, a save's unwritten
reason and an alignment error also ran past a phone's tile, and in the
right-hand column past the window. So `overflow-wrap: anywhere` sits on the
roots of an open volume's panels, `.alignment-panel`, `.metadata-editor`
and `.stamp-comicinfo` (the editor draws its form and its stamp section as
siblings, so it has no single root), the editor's text box is bounded to
its tile, and `PathWithBreaks` still gives a path its first break points.
The rules hold at every width: at 1280 px too the title box is bounded to
the tile's column and long text wraps in it; the panels still sit in their
tile, as the non-goal says. In `classNames.test.ts`, `archives`, `path` and
`unwritten` leave `UNREAD` for `HOOKS` (tests select by them, no rule names
them), and `metadata-editor` and `stamp-comicinfo` leave `HOOKS` (a rule
now names them). Earlier, after the pre-flight, the O229 bullet above was
amended in place: its focus target became the accessibility design's §3.3
target (pre-flight M11), and its thirteen modules became twelve (M10).

*Amended again 2026-11-13, after Task 7:* in the editor's own face, which
the box takes once the tile title's rule no longer reaches the Title field,
the bound still holds the box to its tile at 1280 px and on narrower phones.

*Amended again 2026-11-13, after Task 8:* the Title box is a textarea, as
wide as its tile at rest, and only at rest is it held to the tile; while it
is edited it opens, as the next section has it.

*Amended again 2026-11-13, after Task 8's review:* the build settled these,
among others, beyond the approved design: an Enter that ends an input method's
composition, Safari's among them, sent with keyCode 229, does not save;
Escape puts back the title the box held when it took focus or at its last
successful save, whichever came later; and in the Series page's on-select
mode a pointer entering another tile does not move the panel while focus is
inside the open panel.

## The Title box opens downward (the owner's ruling, 2026-11-13)

Bounded to its tile, the Title box showed a few words of a long title and
scrolled sideways under the caret. The owner ruled that "a long sideways
scrolling bar is a pretty bad experience": while a title is being edited,
the box opens downward into a larger, square-ish box, wider than the tile,
that never runs off the right edge, and it ships in this PR. The design
the owner approved:

- **At rest** the box is one line, as wide as its tile, as Task 5 left it.
- **Opened** when it takes focus (click, tap or Tab), it draws over what
  is below and beside it, and the title wraps inside it. Nothing else on
  the page moves.
- **Size:** about two tiles wide, and as tall as the title up to about
  five lines, past which it scrolls inside. On a phone, at most the width
  of the window less its margins.
- **Edges:** it never runs past the page area's left or right edge, so
  nothing scrolls sideways. Near the right edge it opens leftward, lined
  up with its tile's right edge.
- **Keys:** Enter saves, as Save does. Escape puts back the title the box
  held when it opened and closes the box, not the panel. A pasted line
  break becomes a space: a title is one line.
- **Closing:** the box closes when focus leaves it, and unsaved text stays
  in it, as now.

*My calls, shown to the owner with the design:* the two-tile width, the
five-line cap, what Escape does, and closing when focus leaves. The
non-goal stands: the panel still sits in its tile; the open box is drawn
over the tiles around it only while it is being edited.

*Reopened 2026-11-14 by the owner: the Title field wraps its whole title at
rest, as tall as it up to five lines, and never opens; it saves as it is
left or on Enter, with no Save, and the panel it sits in lies over the rows
below (O283); see `2026-11-14-compact-series-tiles-design.md`.*

## Left to the builder

The CSS values, test shapes and file layout; how ⚙ is made the account
panel's focus target; the wording of O274's narrowed row; and the five-part
status edits for the four rows this closes.

## How it is proven

Each fix gets a test that fails with the fix reverted, run and watched
failing. The phone-width checks (O141, O274, and O171's computed size
under a coarse pointer) run in the browser project at 390 px. A drive on a
scratch copy of the test library checks all five by hand at 390 px and
1280 px.

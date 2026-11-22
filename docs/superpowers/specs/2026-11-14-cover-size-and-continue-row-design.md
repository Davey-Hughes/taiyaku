# Cover size and the Continue row (O285, O288)

2026-11-14 and 2026-11-15. Settled by the owner on prototypes served on
127.0.0.1:4398 from a scratch copy of the test library, in short rounds. The
prototypes as they tried them are the commits on `continue-align` titled
"Prototype as the owner tried it": the first holds the Continue row's
heading, covers and fade, the ones after it the cover-size slider, the
covers' layout, the line under Continue and the View menu's rows. Two
rulings came after the last prototype and are not in it: the layout on
phones (item 9) and the slider as a kind of setting (item 13). This work closes O285 and opens and closes O288. It is
a small change in size: this document says what was settled and what is left
to the builder.

## Intent and what the owner settled

### The Continue row (O288)

On the library's home page (`/`), with the sidebar beside the main column:

1. **The heading lines up.** "the 'continue' and the line next to it are a bit
   high. they should line up with the 'libraries' line on the left". The
   Continue rule was 6.65 px above the sidebar's Libraries rule at every
   width, with or without the web fonts (14.9 px under a coarse pointer). Now
   both rules are drawn on the same pixel row. "it's good".
2. **The covers are the grid's.** "we should adjust the size of the cover in
   continue though, it should be the same as the other ones on this page". A
   Continue cover is as wide as a cover in the grid below it, and its columns
   line up with the grid's, also when more books are in progress than fit
   and the row scrolls (the owner chose "always exact" over a CSS-only row
   that drifts once it scrolls, then "covers look right").
3. **A soft fade says more lies beyond.** "a soft fade would be nice", then
   "the fade could be a bit more obvious", then "yes it's good". The row fades
   to the page over 80 px on each side it can still scroll to, fully clear for
   the last 12 px and half see-through at 50 px; no fade when the row fits.
4. **A line under the row.** "add a line below continue to separate it from
   the rest of the library". The heading's own hairline (`--line`, 1 px)
   again under the row, across the row's width, with 16 px above it and 16 px
   below it to the chips row (All / Multi-edition / Unread). Drawn only while
   the row is: not with the Continue strip turned off in View, not on search
   results, not with nothing in progress.

*Since 2026-11-22 (O417) each readable card also holds `i` on its cover's
bottom-right corner, outside its link, in `button` mode, and `i` opens a
series page's volume panel under the row. It is inside the card, not a child
of the row's list, so the row's pitch, read from the list's first two cards,
and its laying, fade and line are unchanged, and a ring on `i` lifts the
fade as one on the link does. See O417's row in `docs/status.md` and, for
the panel, `2026-11-14-compact-series-tiles-design.md`.*

### Cover size (O285)

The View menu's density switch (Compact / Comfortable) becomes a **Cover
size** slider, on the home page's View menu and a series page's alike.

5. **One size, everywhere covers are listed:** the library grid, the
   Continue row and a series page's volumes. The name and the size in px sit
   on one row, the slider the panel's width under them.
6. **96 to 240 px, the default 144 px at the middle of the track.** "or make
   it stick in the center", then, after trying a catch at the middle: "I
   prefer the catch off, but maybe we can put a small line in the middle and
   leave the double click. also maybe we can make each step in the bar
   smoother", then "ok it feels good". The left half runs 96 to 144 px in
   1 px steps and the right half 144 to 240 px in 2 px steps (97 positions);
   an arrow key moves one position. (The two step sizes are my call: they
   are what puts 144 at the middle with even steps, and every size the old
   switch could store lands on one.)
7. **A small line marks the middle:** 1 px wide and 6 px tall, in
   `--line-strong`, under the track where 144 sits, so it shows with the
   thumb on it.
8. **A double-click resets it.** "we should also make double clicking the
   slider return it to the default". Anywhere on the slider, the thumb or the
   track; the reset saves like any other change. No catch at the middle: the
   owner tried one at two strengths and turned it down.
9. **The covers are exactly the slider's size.** As many columns as fit at
   the least gap, the spare room spread into the gaps between them, and a
   short last row on the same columns. The least gap grows from 0.6rem at
   96 px to 1rem at 144 px and stays 1rem above. The owner chose this
   ("Smooth") over covers that grow to fill the row ("Fill") after trying
   both, told first that when few covers fit the gaps get very wide (a series
   page 1280 px wide at 240 px: 3 columns in Chromium, 4 in Firefox, whose
   page has no scrollbar there) and that at a phone's 390 px and 240 px there
   is one column, at the left.
   **On phones the covers fill the row.** Below the shell's 900 px
   breakpoint the columns are `minmax(size, 1fr)`: as many as fit at the
   least gap, each grown to fill the row, so the slider sets the least a
   cover may be and changes how many columns there are. Told that at
   390 px the default would draw two 144 px covers about 70 px apart
   (worked out from the CSS, not measured), where main draws two about
   171 px wide and 16 px apart, the owner chose "Fill on phones"
   (2026-11-15) over keeping the covers exact or capping the gap. From
   900 px up the covers stay exact.
   *Since 2026-11-16 (O302) the columns below 900 px are
   `minmax(min(size, 100%), 1fr)`, so a list narrower than one cover lays
   one column at the list's width; see O302's row in `docs/status.md`.*
   *Since 2026-11-16 (O305) the phone layout, where the covers fill the
   row, holds below 900 px wide or below 500 px tall; see O305's row in
   `docs/status.md`.*
10. **What was stored carries over.** A stored `compact` becomes 104 px (its
    6.5rem tile) and `comfortable` the default 144 px; the next save stores
    the size and drops `density`. A stored size off the scale is drawn at the
    nearest position's size. A series page's volumes were 120 px at
    Comfortable and are 144 px at the default (told the owner).

### The View menu

11. **Its words cannot be selected.** "can you make sure the text in this
    view box isn't selectable? it's annoying to accidentally select it".
12. **A checkbox's whole row toggles it.** "make the whole row for 'continue
    strip' be clickable for the toggle": the row reaches the panel's edges
    and shows a pointer.
13. **The slider is a kind of setting, not a control drawn by name.** The
    prototype draws it in `ViewMenu.tsx` by the key `coverSize`, against
    C1's §4 ("a browse module needs no settings UI of its own"; the View
    panel is generated from the preferences' types). Asked whether to keep
    it as a named exception or teach the generator a slider kind, the
    owner chose the kind: "We should do generic slider kind and maybe we
    will reuse it in the future" (2026-11-15). A module declares that a
    number preference is a slider, with its scale (the positions, the size
    at each and back), the default it marks at the middle and resets to on
    a double-click, and how its value reads (px here); `FieldControl` draws
    any such preference as this slider, in the View panel and in the
    reader's Settings panel alike, so C1 §4 holds whole and the next slider
    is a declaration.

## Non-goals

- A progress indicator on the Continue cards: that is O132 (the owner's
  2026-09-18 setting *Off / Covers / Progress*), which the owner asked on
  2026-11-14 to take up as a quick follow-up. Its row says so; it is not built
  here.
- Moving the sidebar, the wordmark or the Libraries heading: the wordmark keeps
  its own box (the 2026-10-26 ruling at `Shell.tsx`'s wordmark, and
  `shell.css`'s coarse-pointer block, "not drawn at 44"), so Continue moves to
  meet it.
- Lining the heading up on phones: below the shell's 900 px breakpoint the
  sidebar is a drawer, so there is nothing to line up with. Everything else
  here applies at every width, the covers' layout as item 9 says.
  *Since 2026-11-16 (O305) the phone layout, the drawer's, holds below
  900 px wide or below 500 px tall; see O305's row in `docs/status.md`.*
- A size per page, or one that follows the account across devices (my call):
  one size for every list, kept in the browser as `density` was.
- A double tap on touch (my call): whatever the browser itself reports as a
  double-click resets; nothing is added for touch.
- The catch at the middle and the Fill layout: tried and turned down.

## Constraints

- Nothing above the Continue heading moves: the page head keeps its height
  (its no-shift reason at `shell.css` ~194), and the search box, the search
  results and a home page with nothing to continue are unchanged. What lies
  below the Continue row moves down by the alignment and the line, only while
  the row is drawn.
- The sizes stay in CSS: the slider sets one bare number (`--cover-n`) and
  `library.css` turns it into the track width and the least gap (`--tile-min`,
  `--tile-gap`) for the grid, the Continue row and a series page's volumes;
  any script reads them from the live computed style.
- No React render per scroll frame or resize; the Continue row's gap is in
  place before the first paint. The covers follow a drag of the slider.
- O283's tile panel, two tiles wide, spans the gap the columns are actually
  laid apart by, not the least gap, and the columns' own width on phones.
- The Continue row matches the grid in both layouts: from 900 px up its
  covers are the size and its gap the grid's spread gap; below, its covers
  are the grid's filled column width at the least gap. Either way the row
  keeps them when it overflows and scrolls.
- A cover under the fade still takes a press; a focused card's ring is never
  faded (the fade is off while a card has `:focus-visible`).
- Every theme (System light and dark, Paper, Ink): the fade is a mask to
  transparent, and the lines use the theme's own tokens.

## Approaches

- **Alignment:** a `margin-top` on `.continue h2` built from tokens (the page
  head's height expression hoisted to `--head-row`, the sidebar's padding, the
  seal's height and the two headings' half line boxes), with its own value
  under a coarse pointer. Chosen over moving the sidebar (ruled out above).
- **The covers' layout:** Smooth (exact covers, spare room in the gaps) or
  Fill (`minmax(size, 1fr)`, covers grow to fill the row). The owner chose
  Smooth, then Fill below 900 px for phones (over Smooth there too, or a
  capped gap with the leftover split at both edges).
- **The slider in the View menu:** drawn by its key as a named exception to
  C1 §4, or a slider kind the generator knows. The owner chose the kind.
- **The Continue row's columns:** CSS alone gives the row the least gap,
  which drifts from the grid's spread gap; a small hook (`useContinueRow`)
  works out the grid's gap at the row's width, n = max(1, ⌊(W + gap) /
  (size + gap)⌋) and (W − n·size) / (n − 1), and sets `--continue-gap`. The
  owner chose the exact row; without the script the row falls back to the
  least gap.
- **The slider's middle:** a linear 96–240 scale puts 144 a third of the way
  along; the two-step scale puts it at the middle. At the middle: a catch
  (tried at two strengths) or a mark with a double-click reset. The owner
  chose the middle, the mark and the reset.
- **Fade:** a `mask-image` gradient per side, switched by `data-more-before`
  and `data-more-after` that the same hook sets on scroll and resize. Chosen
  over scroll-driven animations, which Firefox (the owner's browser) does not
  run by default.

## Left to the builder

- Hold the prototype with tests watched failing against a hand-applied
  defect: the two rules' centres equal at ≥ 900 px (fine pointer) and the
  coarse value pinned; phones unchanged; Continue cover == grid cover and its
  columns over the grid's at several widths and sizes, one entry and many,
  after a live resize and a slider change; first paint already exact; the
  fade's sides at start, middle and end, none when the row fits; a press
  through the fade; the focus ring whole; the line under the row present, and
  absent in each case item 4 names (nothing in progress was not driven in the
  prototype).
- The slider: the scale both ways (every old stored size exact, an odd size
  above 144 to its neighbour), the arrow keys, a double-click on the thumb and
  on the track saving 144, the migration (`compact`, `comfortable`, nothing
  stored, a stored size), the mark centred under the thumb at the middle in
  both browsers, the output and `aria-valuetext` in px.
- The layout: covers exactly the size, gaps never under the least gap, a short
  last row on the grid's columns, on the home grid and a series page;
  `tilePanel.ts`'s gap with one column and with many. Below 900 px the
  covers fill the row at the least gap, at a phone's 390 px and just under
  the breakpoint, and the Continue row matches the grid there too; across
  the breakpoint the layout switches without a stale gap or column width.
- The slider kind: declared by the module, drawn by `FieldControl` with no
  key named in the panels, and C1 §4's own test still holding; a second
  declared slider (a test fixture) drawn the same way.
- The View menu: no words selected by a press that moves, and a checkbox row
  toggling from its ends, in Firefox as well as Chromium. In the prototype,
  Firefox does not pass a label's click on to its checkbox once the press has
  moved 3 px or more (Chromium does at every distance driven, 1 to 4 px), so
  a slightly sloppy press on the row's words does nothing there; the row
  should toggle on such a press in both (my call: the owner asked for the
  whole row).
- The prototype's hook registered `--tile-gap` with `@property` so the browser
  hands it over in px; check the fallback path is reachable and tested, or say
  why not. Whether the slider saves on each step of a drag or on release is
  the builder's call; say which.
- The row's own scrollbar, where a browser draws one, fades at its ends with
  the row: accepted, say so in the notes.
- Drive in Firefox as well as Chromium.
- Docs: close O285 and open and close O288 in `docs/status.md` (the five-part
  edit, rebased on #251's O289), a notes section, and every doc that still
  describes `density` or a fixed cover width as current (grep `density`,
  `compact` and `104 px` under `docs/`; the style-pass spec's "a 104 px cover",
  2026-09-17, ~:573, which no ruling of the owner's set, becomes the slider's
  size), and O132's row noting the owner's 2026-11-14 ask.

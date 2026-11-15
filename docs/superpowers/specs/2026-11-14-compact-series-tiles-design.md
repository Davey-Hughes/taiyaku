# Compact series tiles: O283, the rest of O274, and O285 opened

A short design for one web PR, on `compact-tiles` from main dd70ca14. The
owner settled it on 2026-11-14 by trying a throwaway prototype over five
rounds; its code ("O283 prototype") is a spike, not the implementation.
Decisions are marked **(owner)** or **(controller)**; none is open.

## Intent

A volume's grid tile on the Series page carries ↓ (the download) and `i`
on a line of their own under its title, and the panel `i` opens sits in
the tile's column, pushing every row below it down. The owner wants a
compact tile and the panel drawn over the rows below (O283). Over the
rows, the panel can also be wider than the tile's 1280 px column of
about 130 px, O274's open half.

## Goals

1. A tile is its cover and its title; `i` sits on the cover.
2. The open panel lies over the rows below, two tiles wide, holding the
   Title field, the alignment detail and ↓ Download; nothing else moves.
3. The Title field shows the whole title, saves as it is left or on
   Enter, and never grows when clicked.

## Non-goals

- **Trimming `.cbz` from titles.** The prototype strips an archive
  extension in the web only (`shownTitle`, `saveShown`). The fix is O284,
  server-side, with O282 on `shorter-urls`; none of it comes over. (owner)
- **The cover-size slider** in place of the `density` toggle: opened as
  O285, not built. (owner)
- The table layout's missing route to alignment detail (O63) stays open.

## Anti-goals

It fails, however well it works, if any tile moves when a panel opens or
closes, when `i` fades in, or when "Saved" comes and goes (a scrollbar
appearing included); if a panel scrolls the page sideways; if Tab or a
finger cannot reach `i`; if the Title field enlarges or moves when
clicked; or if a changed title is lost without a word as a panel closes.

## Constraints

- **Web only:** `Series.tsx`, `MetadataEditor.tsx`, `styles/library.css`,
  their tests and the docs. No server, API or route change.
- **Targets:** `i` and Download at least 24 px, 44 px under `(pointer:
  coarse)` (`2026-09-17-style-pass-design.md:606-607`; `library.css`'s
  coarse block names `.volumes a.get` and `.volumes .panel-open`). "↓
  Download" is a word button, so 14 px text (`--text-ui`), not a
  one-glyph control's 12 px (`docs/status.md`'s **Text** sentence, in
  the sidebar and readability redesign under Later).
- **Access:** `i` is hidden by opacity, never `display` or `visibility`,
  and keeps `aria-expanded`. Escape is caught on the way down at the
  document and stopped, as `popover.ts`'s `usePopover` does, so the phone
  drawer does not close with the panel. "Saved" is a live region there
  from the first render, outside the label so it is no part of the
  field's name. An Enter or Escape ending an input method's composition
  stays the method's. `i`'s fade goes under `prefers-reduced-motion`.

## Approaches

Offered on 2026-11-14; the owner picked (a). (owner)

- **(a) `i` on the cover, ↓ moves into the panel** (picked).
- (b) ↓ and `i` both on the cover, always shown: on a phone, at 44 px
  each, they cover about half the cover's top edge.
- (c) Both on the cover, shown only on hover or focus, always on touch.
- (d) Both beside the title: the title loses about 50 px, 88 px of a
  113–171 px tile on a phone.

In round 2 the owner took (c)'s hover rule for `i` alone.

## The design as decided

    +-----------+     +------------------------+  4 px under the tile,
    |        ⇄  |     | Title          Saved   |  over the rows below
    |   cover   |     | [whole title, ≤5 lines]|
    |      [i]  |     | alignment detail       |
    +-----------+     | [↓ Download]           |
    Title             +------------------------+

**The tile** is its cover and title, its link as now. (owner) **`i`** sits
on the cover's bottom-right corner, nearest the panel it opens, 0.25rem
in from both edges at any column width (owner, round 5), over the cover
but outside the tile's link. Where every pointer can hover it is
transparent and takes no pointer until its tile is hovered or holds
focus, or its panel is open; Tab still reaches it. With any touch input
it is always shown (owner, round 2). "Any touch input" is the
controller's reading: a touch laptop's primary pointer hovers, so
`(hover: hover)` alone would leave its finger tapping an invisible `i`
and landing on the cover's link; the hiding rule therefore also requires
that no pointer is coarse (`not (any-pointer: coarse)`). The prototype
fades it over 120 ms. **⇄** moves from the
cover's bottom-right to its top-right in the grid, so it never sits under
`i`; the table's inline ⇄ is unchanged. (controller)

**Placement.** Under its tile, over the rows below, so nothing moves as
it opens; two tiles and the gap wide, from the tile's left edge, or back
from its right edge in a row's last column (owner). Never past either
edge of the page area nor wider than it, as the owner ruled for the Title
box on 2026-11-13 (`2026-11-13-phone-fixes-design.md:146-151`), and laid
again when the window or the page area changes width. Above the covers
and their marks, under the phone bar and View, and drawn as View's panel
is: 12 px padding, a `--line-strong` hairline, `--surface`, its shadow.

**Closing** in `button` mode: on Escape, a press outside the tile and its
panel, or `i` again (owner). Escape sends focus back to `i`. An Escape in
a focused Title field first puts its title back; the next one closes the
panel. It also closes when focus arrives outside, as `usePopover` does
(controller; settled item 6). No close loses an edit (settled item 3).

**Contents**, top to bottom, 12 px apart: the Title field, the alignment
detail, ↓ Download (owner, round 3). The stamp section, which the editor
draws after its form, falls between the Title and the alignment detail.
In the panel the alignment detail drops its own border, margin, padding.

**The Title field** (owner, rounds 2–4): the label, then 4 px below it a
field as wide as the panel with 8 px side padding (`--space-2`), wrapping
the whole title at rest as while edited, as tall as the title up to five
lines and then scrolling, re-fitted as the title is typed and as the
panel's width changes. It never enlarges, opens or moves when clicked:
O274's `layOpen` widening goes. It saves when left or on Enter. Save
goes, and so does `p.current`, the paragraph under Save repeating the
title the server last answered (`MetadataEditor`'s `current` state).
"Saved" shows muted at the end of the label's line, taking no row. Escape
puts back the stored title, the one held when the field took focus or at
its last successful save. Failures and unwritten reasons stay under the
field, 8 px apart; a pasted line break is still a space.

**↓ Download** is a word button at the panel's foot, with today's `href`,
bare `download`, `aria-label` and guards (`mayDownload`, a shown edition,
`missing === 0`), its label optically centred because the ↓ is drawn
narrower than its advance; the prototype takes 2 px off its left padding
and adds 2 px to its right (owner, round 4).

**↓ where a tile has no `i`.** With `panel: 'off'`, and in `on-select`
mode, a tile has no `i`, and on a phone an `on-select` panel cannot be
reached (the tap that focuses the link follows it). There ↓ takes `i`'s
place on the cover's bottom-right corner, at `i`'s size and inset, shown
by `i`'s rule below, with today's `href`, `download`, `aria-label` and
guards; it is the only download such a tile has, so an `on-select` panel
does not repeat it. (owner, after review) So every mode keeps a download
in the grid, and no tile has more than one control on its cover.

**`on-select` mode** keeps its overlay and gains `button`'s closes:
Escape (focus back to the tile's link) and a press outside the tile and
its panel. A pointer leaving the tile does not close it, and a pointer
entering another tile moves no panel while focus is inside one, the
build-settled rule (phone-fixes `:129-131`). (controller)

**The table layout** is unchanged: its download links, its inline ⇄, no
`i` and no panel.

**The scrollbar gutter.** `scrollbar-gutter: stable` on what scrolls the
Series page while it draws the grid: the page area (`.library > main`)
from 900 px up, the document root below, and only where `.volumes` is
drawn. A panel running past the end of a page that did not scroll adds a
classic scrollbar, which narrows the grid and moves every tile.
(controller; the prototype added it unasked)

**Phone and desktop.** Touch always shows a 44 px `i`, a hover pointer a
24 px one on hover or focus. At 390 px the panel spans the page area from
either column; on a desktop it is two columns wide.

## Rulings this reverses or bends (pre-flight)

- **The 2026-09-18 settlement** that a volume's panels fit inside its
  tile (`2026-09-17-style-pass-design.md:457-466`), kept as a non-goal in
  `2026-11-13-phone-fixes-design.md:29-32` and `:160-161`, is reopened by
  the owner's own O283 ask. `library.css`'s comment on `.alignment-panel,
  .metadata-editor, .stamp-comicinfo` cites it and must change.
- **The owner's 2026-11-13 Title box ruling** (phone-fixes `:142-156`):
  one line at rest, opening on focus, "Enter saves, as Save does", unsaved
  text kept when focus leaves. The Title field above keeps Enter's save
  and reverses the other three.
- **Download §8** (`2026-08-23-library-download-design.md:424-434`,
  `:448`): a download in the panel "would be a download some readers have
  switched off", so one control per tile; "The panel gains nothing".
  Reversed by the owner's pick (a); §8's worry is met by ↓ taking `i`'s
  corner wherever there is no `i` (above), so still one per tile.
- No closed row in `docs/status.md` rules on these controls.

## Status-ledger effects

- **Close O283** with the five-part edit.
- **Close O274.** Checked against the prototype: the panel holds the
  whole editor, stamp section included, in `button` and `on-select` mode
  alike. Rewrite its row and its bullet.
- **Open O285**, the owner's of 2026-11-14: a cover-size slider in place
  of `density`, whose compact sets `.library.compact .grid` and
  `.volumes` to `minmax(6.5rem, 1fr)` against `.volumes`' 7.5rem
  (`Shell.tsx` sets the class). O284 is taken on the O282 branch.
- **Record the reopened settlement** in O283's bullet, and give both
  specs above a dated pointer here, their prose kept.

## Settled after review

The draft's open points and a builder's read, settled. Only the first
was the owner's; the rest are the controller's calls, each reversible by
the owner.

1. **↓ where there is no `i`**: on `i`'s corner (owner; above).
2. **How `on-select` closes**: as `button` does, not on the pointer
   leaving (controller; above).
3. **No close loses an edit.** Every way a panel closes (Escape outside
   the field, an outside press, `i` again, focus arriving outside, the
   pointer choosing another tile) first saves a changed title, not
   relying on the press to blur the field, since a browser need not focus
   a pressed button. A save that fails keeps its panel open, or reopens
   it, with the failure or unwritten reason shown under the field. A
   closed editor never swallows a failure. (controller)
4. **A title changed while a save is out** is sent once that save
   returns, if it still differs from the title that save stored; one save
   at a time, the newest text last. The prototype dropped it, which the
   anti-goals count as a failure. (controller)
5. **A blank title**: leaving the field, or Enter, with only whitespace
   puts the stored title back and sends nothing. The server accepts
   `title: ""` unchecked (`metadata.rs`'s `patch_work`), and with no Save
   button a stray click after clearing must not write it. (controller)
6. **Prototype behaviour no ruling names, kept as the owner tried it**:
   an unchanged title is not sent; "Saved" clears after 3 s and on the
   next edit; focus arriving outside closes the panel; the field keeps the
   typed text, not the server's re-projected title; the panel always opens
   downward and is not scrolled into view; leaving the field for the stamp
   controls saves a changed title. (controller)
7. **`i`'s name** becomes "details for …" (the panel now holds the title
   and the download as well as alignment). (controller)
8. **The reference for "the prototype does"**: its round-5 diff against
   dd70ca14 is kept at
   `.superpowers/sdd/compact-tiles-evidence/prototype-round5.diff`
   (gitignored, local; 638 lines). It is a spike: read for fades,
   offsets and ordering the rulings leave unstated, never copied.

## Settled after the drive

The hand drive of d08d1aee found three things working as built that the
owner wants changed before the branch merges (2026-11-14). A row moving
when a saved title gains a line on its tile stays as built: the title
changed, and the anti-goals name only a panel, `i` and "Saved".

Each had one sensible way. Item 9 follows the owner's ruling of
2026-10-21 on waits (`2026-10-20-config-grouping-design.md`, a wait is
"drawn waiting only once the wait has lasted 200 ms"): "Saving…" from
the send would flash on every quick save, and one shown only while a
close waits would leave a slow save with no close unexplained. For item
11, padding added to the grid by script when a last-row panel opens
would give the same space with more code than a box below the panel.

9. **A slow save says so.** Once a title save has been out 200 ms, the
   "Saved" note's place says "Saving…" until it lands, then "Saved" as
   now; a save that lands sooner shows only "Saved". It is the same live
   region in the same place, taking no row, so nothing moves as it comes
   and goes, whether the panel stays open or a close waits on the save.
   (owner approved)
10. **Escape clears a failure.** The Escape in the field that puts back
    the stored title also takes away the failure under the field, which
    spoke of the title Escape discarded. The note that a save landed but
    was not written beside the library stays: it speaks of the stored
    title. (owner approved)
11. **A last-row panel ends the page with room below it.** Scrolled to
    the end, the panel has as much space below it as the last row has
    with no panel open, not its foot on the viewport's edge. The space
    only lengthens what the panel adds to the page, and moves no tile.
    (owner approved)
12. **A leave back at the stored title clears a failure too.** A leave,
    an Enter or a close's flush that finds the field at the stored title,
    whether left blank and put back or typed back by hand, sends nothing
    and takes the failure away as item 10's Escape does, leaving the
    unwritten note. The failure spoke of a title no longer in the field,
    and the panel already counts that field as holding nothing unsaved.
    (owner, 2026-11-14, before merge; raised by a review, not by the
    drive)

## Left to the builder

The code's shape (perhaps sharing `usePopover`'s close rules); deleting
`layOpen`, its page-area watch and the `current` state if unused; CSS
values the rulings leave free; test shapes; comments citing O274's open
box or the settlement; the status edits.

## Tests whose assertions change

In `web/tests/`. *Vacuous*: would still pass, asserting nothing, since ↓
is absent at rest or the fixture mounts the editor in a bare tile.

- `Series.test.tsx`: "offers a download per tile, …"; "keeps the download
  anchor outside the tile link, …"; "withholds the per-tile download …",
  "shows no download at all when the account may not download" and "shows
  no download when no gate sits above the screen" (vacuous); "keeps the
  series it drew, and the editor open, beside the alert …" (`.current`).
- `MetadataEditor.test.tsx`, pressing Save: "saves a corrected title and
  shows the new one"; "shows the server-reprojected title, …"
  (`p.current`); "says so when the correction could not be written …";
  "says nothing about being unwritten …"; "renders exactly what an
  ordinary successful save renders …"; "names every reason a fanned-out
  edit …"; "surfaces a hard failure as an alert, …"; "stringifies a
  rejection …"; "clears the last save failure …" (`.current`); "sends no
  second save for an Enter while one is out, …"; "puts a pasted title in
  at the caret, …"; "turns each line break that reaches the box another
  way, …". On leaving: "keeps the typed title when focus leaves the box";
  "puts back on Escape the title the box held when it took focus, …" (its
  blur now saves); "watches the page area only while the box is open, …".
- `MetadataEditor.browser.test.tsx`: "draws its label and box in the
  editor’s face, …" (`.current`); the four tests in "a stamp plan drawn
  in a volume’s tile (O274)" (vacuous; the unwritten-reason one presses
  Save); every test in "the Title box opens downward while it is edited
  (O274)" but the paste one.
- `MetadataEditor.scrollbars.browser.test.tsx`: "is as tall as its
  title’s lines at its open width".
- `Series.onSelect.browser.test.tsx`: "keeps the panel, the focus and the
  restored title on Escape with the pointer on the box’s overhang" and
  "keeps the panel and the typed title on Tab to Save …" (no overhang, no
  Save); "reveals the volume under the pointer again …" (settled item 2).
- The grid download tests above also gain the no-`i` cases: ↓ on the
  corner with `panel: 'off'` and in `on-select`, and none in its panel.

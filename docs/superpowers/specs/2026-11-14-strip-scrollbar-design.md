# The strip's progress line as a scrollbar (O286)

2026-11-14. Settled by the owner over six rounds of a prototype served on
127.0.0.1:4399 from a scratch copy of the test library; the prototype as they
tried it is commit 6ebbebbc on `o286-strip-scrollbar`. This document says what
was settled and what is left to the builder; the plan holds the prototype with
tests.

## Intent

The owner asked to "make the scroll bar for the filmstrip usable to scroll …
like click and move", and then: "I want it to act like a regular scrollbar,
but with the style we gave it". The line under the reader's strip
(`Chrome.tsx`'s `.reader-progress`, with the reading-progress fill and the mark
O216 added) shows where the rail stands but takes no press. O138's ruling
(2026-10-11) gave the rail no scrollbar of its own; that stands. What O138
said it cost, "the bar's thumb was a mouse's fast way across a long volume",
is what this gives back, on the line that is already drawn.

## Goals: what the owner settled, round by round

1. **Drag and jump** (the owner's pick "Mark + jump on press", read by them as
   a regular scrollbar). A press on the mark and a move scroll the strip, the
   mark staying under the pointer at the point it was taken. A press on the
   line away from the mark moves the strip so the mark is centred on the
   pointer, and the same press goes on as a drag. Past either end it stops at
   the end. No glide after the release: a scrollbar does not coast. A press
   stops a glide the rail is running.
2. **Mouse and trackpad only** (the owner's pick). Touch and pen keep swiping
   the strip; nothing on the line answers them.
3. **The mark is sized like a browser's thumb, within bounds** (round 4: "it
   should be closer to a browser", then the pick "Browser, capped"): the share
   of the volume the strip shows, times the line's width, never under 64 px and
   never over 160 px. This replaces O216's "an eighth as wide as the share …
   never under 32 px"; the cap is why October's "quite huge" does not come back
   on a short volume. At 1280 px: 64 px for 237 pages, about 131 for 99, 160
   for 32.
4. **The line wakes on a mouse move** (round 5: "in a browser the scroll bar
   gets bigger as you move the mouse at all over the page, which we should
   mimic"). Any mouse move over the reader while the bars are up draws the line
   thicker; it goes back 1 s after the last move, except while the pointer
   rests within 16 px above or below the line, anywhere across the bar's
   width, or a press holds it.
5. **Thicker, but not much** (round 4: "visually it's a bit thick, we should
   keep the grabbable area but make it visually thinner"): awake, the line is
   4 px and the mark 8 px, from 2 and 4 at rest, both centred where the line
   is. Nothing else in the bar moves by a pixel.
6. **A forgiving press area, the same size** (round 3: "I don't want the overall
   area to be bigger but once you're on it it should be bigger until you
   definitely leave it"): only a 10 px band centred on the line takes a press.
   It covers neither a tile nor the readout's text. The 16 px keep zone of goal 4
   decides only how the line is drawn and takes no press.
   *My call:* the band and the wake exist wherever the device has a fine
   pointer (`any-pointer: fine`), so a touch laptop's trackpad gets them too;
   only a `mouse` pointer is answered. The band's top 4 px lie over the rail's
   bottom padding, below the tiles, so that a press on the top of the drawn mark
   takes the mark and not the rail's own drag (which would move the strip the
   other way); on a touch laptop a finger landing in that 4 px strip therefore
   does not swipe the strip. The test support's pointer table
   (`web/tests/support/pointer.ts`, `HOLDS`) learns the new conditions; O283's
   `(hover: hover) and (not (any-pointer: coarse))` is for a control a finger
   could not otherwise reach, which this is not.
7. **The motion** (round 5: "it would also be nice if the animation was a bit
   better"): wake over about 150 ms easing out, sleep over about 300 ms easing in
   and out, the line, the mark and the fill moving as one; none of it when the
   reader's animation setting is off or the system asks for reduced motion.
   *My call:* one registered custom property drives all three; a browser
   without `@property` (Firefox before 128; the build targets `firefox120`)
   draws the change at once instead of easing, which is accepted.
8. **A rounded end on the red fill** (round 6, "the slight rounding is
   better"): the fill's leading end, at the current page, is a half circle of
   its own height (1 px radius at rest, 2 awake); its start and the grey line
   stay square. In a work read right to left the round end is on the left, with
   the mirrored line.
9. **Ride-along:** the Title field's failure text (`MetadataEditor.tsx`'s
   `.failure`, inside `.metadata-editor`) is drawn in `--danger`, as
   `.reader-failure` and `.account-failure` are. No rule coloured it (seen on
   the compact series tiles' third drive).

## Non-goals

- A drawn scrollbar on the rail (O138 stands).
- Touch or pen on the line (goal 2).
- Keys: a scrollbar takes no focus, and the strip already has its keys. The
  line stays `aria-hidden`.
- Firefox's second, bigger step when the pointer is right on its bar: offered,
  not asked for.
- A press on the line paging toward the press, as some platforms do: the owner
  chose jump.

## Anti-goals

- A press that turns a page, opens a tile, selects text, or leaves focus where
  Space or an arrow then does something other than before the press.
- A drag the bars' hiding cuts off, or a listener left behind when the bars
  hide or the reader closes. *My call:* the bars hiding during a hold (Escape
  can) ends the hold as a release would, with no glide, and the line comes back
  at rest when the bars do.
- The mark lagging the pointer, or the line, mark and fill out of step while
  they grow or shrink.
- Any layout shift in the bar.

## Constraints

- The line is mirrored with `scaleX(-1)` for a work read right to left, and
  the rail is `row-reverse` with a negative `scrollLeft`; pointer coordinates
  are the screen's, so where a press falls is measured from the line's drawn
  start.
- The rail's `scrollWidth` changes while a press is held (thumbnails load; the
  window rebuilds near a spacer), so the scroll a pointer asks for is worked out
  again from the current widths each time the strip moves, not only when the
  pointer moves.
- No React render per pointer move or per scroll frame from the line's own
  state (O216's lesson). The strip's own window rebuilds as a scroll carries it
  near a spacer stay as they are for the wheel and the rail's drag; a drag on
  the line moves the rail about 20 px per pointer pixel on a long volume, so it
  meets them more often, and the plan measures that rather than assuming it.

## Approaches

1. **Handlers on the line in `Chrome.tsx`, one "scroll the rail to this point"
   from `Filmstrip.tsx`.** The mark keeps following the rail by the path it
   uses now (`tellShown` → `placeMark`). Chosen: the line and its approved look
   stay where they are.
2. Move the line into `Filmstrip.tsx` so one component owns rail and line:
   tidier, but it moves the line, its look and about 30 tests.
3. An invisible native scrollbar over the line: its thumb would be the
   browser's own size and place, so the drawn mark and the hand would part.

## Left to the builder

- Hold the prototype with tests watched failing, with a real mouse
  (`web/tests/support/mouse.ts`) in both reading directions, on a long volume
  and a short one; check the built app as well as dev, and drive it in Firefox
  as well as Chromium (the suite is Chromium only; the owner tried all six
  rounds in Firefox, as they did O216's).
- Chase the one sample in the prototype's checks that left the mark 4.4 px from
  the pointer during a drag (once, under load ~25, not reproduced in five
  targeted drags) before calling it noise.
- The prototype's `placeMark` reads a ref declared later in the component;
  order it plainly.
- Tests that pin what the prototype changed are rewritten to the new behaviour,
  each watched failing against the old: O216's width rule (the 32 px floor and
  the eighth); the mark's insets, now in CSS (`Chrome.test.tsx` "the mark on
  the progress line", ~:378); the single `.reader-progress` rule
  (`reader.css.test.ts` ~:297); and "is drawn in the row above the line and the
  row below it" (`Chrome.browser.test.tsx` ~:451), whose `elementFromPoint` now
  meets the press band. The rail's own drag, wheel and glide tests stay as they
  are.
- Docs: close O286 (the five-part edit); say that O216's width rule was
  replaced, by whom and when, in its row, its notes section and its spec
  (`2026-10-12` §2.1 "How wide" and "How tall"; §3's out-of-scope "a thumb that
  can itself be dragged, or a click on the line that jumps"); and in O138's row,
  that what it said the ruling cost is given back on the line.

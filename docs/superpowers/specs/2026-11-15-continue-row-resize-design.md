# The Continue row keeps its place across a resize, and a cover never spills sideways (O301, O302)

The lane 2 follow-up: one small PR on top of #254. The owner agreed to
bundle the two items, approved the overflow fix, and approved this design
as proposed in the session ("let's do the lane 2 stuff").

## Why

- **O301.** Since O288 the Continue row's pitch follows the list's width in
  both layouts. A cover-size change puts the row back where it was, and a
  window resize does not, so the card at the row's start edge drifts, as
  when a phone turns from 390 to 844 px. A kept place can also outlive a
  change in how many volumes are in progress (O301's row has the sequence),
  and two parts of the place have no test.
- **O302 (new).** Below 900 px the grid's columns are
  `minmax(var(--tile-min), 1fr)`, so a list narrower than one cover (240 px
  covers in a window under about 272 px, or a browser zoomed until it is
  that narrow in CSS px) overflows sideways. `docs/notes.md`, "O285 and
  O288", left it as a question for the owner. The owner approved
  `minmax(min(var(--tile-min), 100%), 1fr)`.

## Goals

1. A window resize keeps the Continue row's place by the rule a cover-size
   change already follows. If the row was scrolled to its end, it stays at
   its end. Otherwise the card at its start edge stays there, at the same
   fraction past its edge, as far as the row reaches. This covers any
   change in the list's width, including the sidebar coming and going at
   900 px.
2. A change in the number of cards in progress drops a kept place, so the
   next size change or resize starts from where the row is seen, not from a
   place recorded before the count changed.
3. Tests hold the two untested parts of the place: a row that fits never
   counts as at its end (the `end > 1` in `atEnd`), and the offset into
   the start card is scaled to the new pitch when that card starts part-way
   past the row's edge.
4. Below 900 px a cover is never wider than the list. On the home grid and
   a series page's volumes, the column's least width is the smaller of the
   slider's size and the list's width.

## Non-goals and anti-goals

- No change to how a cover-size change keeps the place. That behaviour
  was approved on #254, and the resize copies it.
- `sizes="auto"` is not taken, and the owner has not ruled on it. It was
  recommended against in the belief that only Chromium honours it; Firefox
  has honoured it since version 150, and Safari, iOS included, does not.
- Anti-goal: React renders on a resize or a scroll. The row stays laid out
  from outside React and before paint, as `useContinueRow` does now.
- Anti-goal: a place worked out again from the row's own `scrollLeft` at
  each resize step. It reads back rounded or clamped and would drift over a
  window drag's steps. That is why the place is kept rather than re-read
  (the comment at `continueRow.ts` `mark`).

## Approaches

**O301, picked: put the row back from the `ResizeObserver` callback.**
The callback (`resized`) already re-lays the row on every change in the
list's width. Today it then re-marks the place under a `scrollLeft` that
has drifted. Instead it takes the kept place first, lays the row, and
restores the place by the same step the cover-size path uses (one shared
function), then marks.

Considered and dropped:
- A window `resize` listener. It misses the list's width changing without
  the window (the sidebar's column at the 900 px turn), which the observer
  already sees.
- Re-reading the place at each resize. This is the drift the kept place
  exists to prevent.

**O302, picked (owner-approved): `min(var(--tile-min), 100%)` as the
column's least width** in the below-900 px block. The other way was to
leave it and document it. The Continue row already gives a lone card the
list's width (`filledColumn` with one column), so the grid now agrees with
it.

## Decided

- O302 is a new row for the overflow, opened and closed in this PR
  (lane 2 holds O301–O309, and the first-scan lane numbers from O310).
- The notes' question ("O285 and O288", "A question for the owner at the
  PR") records the ruling and what shipped.
- O301 closes by the five-part edit.
- No prototype. The resize behaves the way the size change the owner
  already tried does.

## Constraints

- Each new test is watched failing against the defect it names, hand
  applied: the resize restore removed, the count check removed, `end > 1`
  removed, and the offset left unscaled.
- The resize case needs real layout, so it belongs in the browser project
  (`web/tests/Library.continue.browser.test.tsx`), which CI does not run.
  The local gate covers it.
- `library.css.test.ts` ties the CSS to `library.ts`'s `coverSizes`. The
  image `sizes` attribute may now over-pick below about 272 px, which is
  harmless (a larger image than needed) and needs no change there.

## Left to the builder

- The shape of the shared restore step and where the count lives in the
  place.
- Which tests are hook tests (`web/tests/continueRow.hook.test.tsx`) and
  which are browser tests, beyond the resize case above.

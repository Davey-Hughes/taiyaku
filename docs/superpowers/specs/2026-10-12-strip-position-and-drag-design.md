# The strip's position mark, and dragging the strip — design

**Date:** 2026-10-12. **Branch:** `strip-position-and-drag`, from `main` at
`1eb5723e`. **Closes:** O216. **Opens:** one row, for a defect found on the
way (§5).

O216 was opened by O138's review with two things unmeasured and one decision
unmade. The user took the two readings by hand on 2026-10-12, asked the same
day for two things in answer to the decision, chose the form of the first
when it was offered, and settled its width and its transparency on a
prototype (§1). This spec writes down what they chose, so that it can be
built with its tests.

---

## 0. What exists, read on 2026-10-12

Read at `1eb5723e`.

- The strip's rail (`Filmstrip.tsx`, class `reader-strip-rail`) draws no
  scrollbar since O138. A vertical wheel scrolls it, a finger scrolls it as
  the browser scrolls anything, and clicking a tile turns to its page.
- The rail's scroll listener already computes how far along its travel the
  rail is, once a frame, to decide when to rebuild its window of tiles.
- Under the strip `Chrome.tsx` draws the progress line (`reader-progress`,
  2 px, `aria-hidden`), filled from the left to the fraction of the edition
  read (`reader-progress-fill`). The fill starts at the left in both reading
  directions, while the strip runs right to left for a work read that way.
- Nothing says where the strip stands once it has been scrolled away from
  the page being read.

## 1. What the user found and chose, 2026-10-12

In Firefox, on `main`'s build served locally from a scratch copy of the test
library:

- **A notch of the wheel moves the strip "just over 1 page", and "it feels
  fine".** Those are O216's two readings: Firefox, and a wheel under a hand.
  The step stays as it is.
- **"there's no indication of how far along in the strip we are (no scroll
  bar)".** Offered three forms, the user chose a mark on the progress line,
  to see it, with a thin thumb under the tiles as the fallback.
- **"it would be nice if you could click+drag with a mouse to scroll the
  strip".**

Then, on a prototype of both, over four rounds:

- The mark works. As first built it was as wide as the share of the volume
  the strip shows, which the user called "quite huge", meaning wide.
- It should let the progress line show through it: "some amount of opacity".
- A fixed 32 px "is ok for a volume this size", and it "should potentially be
  scaled larger for a short volume".
- At an eighth of the share shown and never under 32 px: "the scroll bar
  width is great now".

The user did not remark on two things the prototype did, and this spec keeps
both. They are named in §2 so that approving this spec rules on them: the
line is mirrored for a work read right to left, and a drag is a mouse's only.

## 2. What changes

### 2.1 The mark

- **Where.** On the progress line, as a child of it, so it is `aria-hidden`
  with the line. It is drawn while the bars are up and the rail overflows,
  and not at all where the strip shows the whole volume.
- **What it says.** How far along its travel the rail has been scrolled: at
  the start of the volume the mark is flush with the start of the line, and
  at the end, flush with its end. It does not say how much the strip shows.
- **How wide.** An eighth of the share of the rail's whole length that the
  strip shows, and never narrower than 32 px. On the 237-slot volume the
  prototype was tried on that is 32 px at a 1,440 px and a 2,560 px window;
  on a 48-page volume it measured 45 px and 144 px.
  *Replaced 2026-11-14 by the owner (O286): the share of the volume the
  strip shows, never under 64 px and never over 160 px; see
  `2026-11-14-strip-scrollbar-design.md`.*
- **How tall.** One pixel proud of the 2 px line above and below.
  *Replaced 2026-11-14 by the owner (O286): still one pixel proud at rest,
  and where a mouse wakes the line or holds it, the line is 4 px and the
  mark 8 px, centred where the line is; see
  `2026-11-14-strip-scrollbar-design.md`.*
- **Its colour.** The Ink text colour at half opacity, so the fill shows
  through where the mark lies over it. A fourth fixed token beside the
  reader's own three in `tokens.css`, held to the Ink pair by the test that
  holds those: the reader's bar is dark in both themes, and §5's defect means
  a `light-dark()` token cannot be trusted to say so in a built stylesheet.
- **The line runs the way the strip runs.** For a work read right to left
  the whole line is mirrored, so the mark sits under the tiles it stands
  for. **The fill is mirrored with it**: in such a work the read-so-far fill
  grows from the right, where today it grows from the left. Unmirrored, the
  mark moved the opposite way to the strip.
- **When it is told.** `Filmstrip` tells `Chrome` where the rail stands, and
  only when that has changed: after a scroll (in the frame the existing
  listener already takes), after a render, and when the rail's own width or
  its content's width changes with neither, as when a thumbnail loads and a
  tile takes its real width. The prototype missed the last of these, and a
  mark was measured a few pixels stale because of it.

### 2.2 The drag

- **A mouse only**, primary button. A finger and a pen scroll the rail as
  they do today.
- **A press becomes a drag once it has travelled 5 px along the rail.** From
  there the rail follows the pointer one to one, in either reading
  direction, and keeps following outside the strip (the pointer is captured).
  The cursor is `grabbing` while it does.
- **A drag never turns the page.** The click a release would otherwise be is
  swallowed, whether it would have landed on a tile or on the rail.
- **A press that travels less is the click it always was.**
- **No momentum.** The rail stops where the mouse lets go.
- The browser's own drag of a thumbnail image is prevented on the rail,
  since it would cancel the pointer.
- The listeners are attached by hand in an effect, as the wheel's are: the
  rail is a plain element, and the click has to be stopped before React's
  listener at the root hears it.

## 3. Not in this PR

- A thumb that can itself be dragged, or a click on the line that jumps the
  strip there. The user asked for where the strip is, and for a drag of the
  strip.
  *Replaced 2026-11-14 by the owner (O286): a mouse drags the mark, and a
  press on the line away from it centres the mark on the pointer and goes
  on as a drag; see `2026-11-14-strip-scrollbar-design.md`.*
- Momentum, and any change to the wheel's step.
- The defect in §5.

## 4. Tests

Each is to be watched failing against its defect put back by hand.

- **`tiles.test.ts`** (node): the rail's share from its three numbers: at
  the start, in the middle, at the end, with a reversed row's negative
  `scrollLeft`, past either end, and where nothing overflows.
- **`Chrome.test.tsx`** (jsdom): the mark's place and width from a share it
  is told, the floor and the eighth each deciding a case; no mark where the
  strip shows everything; the line mirrored for a work read right to left
  and not for one read left to right.
  *The floor and the eighth were replaced on 2026-11-14 (O286), and these
  cases with them; see §2.1.*
- **`Filmstrip.test.tsx`** (jsdom): the rail's standing told after a scroll
  and not told twice for the same standing; a press and a move past the
  slop scrolls the rail and the click after it reaches no tile; a press and
  a move short of the slop is a click that turns the page; a touch pointer
  is left alone; the same in both reading directions.
- **The browser project** (a real layout; `Filmstrip.browser.test.tsx`, and
  a browser test of the bottom bar, which has none yet): a real mouse drag
  moves the rail and leaves the page where it was, and a real click still
  turns it; the mark stands at the start and at the end of the line with the
  rail at its two ends, in both reading directions; and the mark follows a
  thumbnail's late width.
- **`index.css.test.ts`**: the new token against the Ink pair.

## 5. Found on the way: a built stylesheet gives the reader the light theme's tokens

Not fixed here. It gets a row of its own.

Vite's CSS minifier compiles `light-dark(a, b)` into a pair of `var()`
fallbacks switched by two custom properties. The tokens are declared on
`:root`, so they are resolved there, and `.reader`, which sets
`color-scheme: dark`, inherits them already resolved. Measured on the built
app in headless Chromium 151 with a light colour scheme: inside `.reader`,
whose computed `color-scheme` is `dark`, the progress line is
`rgb(214, 204, 185)`, the bottom bar's text colour `rgb(35, 30, 23)` and the
readout's `rgb(107, 97, 81)`, which are the Paper halves. With a dark colour
scheme they are the Ink halves. `reader.css`'s first comment says the reader
resolves every token to its Ink half, measured in two browsers; that holds
for a stylesheet that is not minified, which is what the dev server and the
tests load.

## 6. The documents

- `docs/status.md`: O216 closed with the two readings and the choice; the
  new row for §5; the counts.
- `docs/notes.md`: a short section with what the user saw, the prototype's
  measurements and §5's readings.

## 7. Corrected while it was built, 2026-10-12

**§2.1 gains a rule: the mark is moved without a render.** As prototyped,
the strip told `Chrome`, `Chrome` kept what it was told in state, and so
`Chrome` and the whole strip rendered on each frame of a scroll that moved
the mark, which is the cost the strip's own scroll listener is throttled to
avoid. `Chrome` now
writes the mark's place and width on the element itself, and a test holds
that a scroll moves the mark and commits no render.

**§2.1's "only when that has changed" means exactly that.** The prototype
left a movement under a two-thousandth of the rail's length untold. The mark
is placed by the rail's travel, which on a volume a little longer than the
strip is wide is a small part of its length, so there the mark could stop
short of the line's end: 31 px short, as the task's review measured it at a
2,400 px window on 21 pages.

**§2.2's sentence about a thumbnail's drag does not describe what
happens.** A press on a thumbnail starts no drag of the image, since the
image takes no pointer events. What the rail prevents is the browser's drag
of a selection, which does cancel the pointer.

**§2.2 gains three rules, each from something a test or a review found.**

- A press ends where its button comes up, on the strip or elsewhere in the
  page: the release is listened for on the window, and a move with the
  primary button up ends a press too. A release outside the browser's
  window was not read. As prototyped, a press let go off the strip was left
  standing, and the rail then followed a mouse with no button down. The
  pointer's leaving the rail ends nothing: a press that slips off the
  strip's edge and back is still a press.
- When a press becomes a drag, focus that the press moved onto a tile of
  the rail goes back to where it was before the press. A mouse press focuses
  the tile under it, the click after a drag is swallowed and so turns to no
  page, and Space or Enter on a tile left with focus would turn to it. A
  press between two tiles moves focus to nothing of the rail's, and that is
  left as it is.
- The travel that makes a drag is measured along the rail only, and a press
  on the rail between two tiles drags as one on a tile does. Both were true
  of the prototype and held by no test.

**§2.2's last sentence gives the wrong reason.** React's own capturing
listener at the root hears a click before the rail's does. What the rail's
listener stops is the click's way on down to the tile and back up, which is
where React calls the tile's `onClick`.

**§4.** The bar's browser cases are a file of its own,
`Chrome.browser.test.tsx`, and the whole reader's are in
`App.browser.test.tsx`. The real mouse they share is
`web/src/testing/mouse.ts`. Two of §4's phrases are not what was built. The
rail's share is held for a `scrollLeft` past the end of its travel, and not
past its start, which is the status row's "Not held". And `Chrome.test.tsx`
asserts the width as the CSS it writes, floor and eighth together; which of
the two decides a width is held where there is a layout, in
`Chrome.browser.test.tsx`.

## 8. Momentum, asked for once the drag had been tried, 2026-10-12

§2.2 and §3 say a drag has no momentum. The user tried the built drag and
said: "strip dragging works, but there's no momentum. is it possible to add
this? and make sure it doesn't conflict with any mobile native scrolling for
this". A prototype of a glide was served the same way, and of it the user
said "yeah it feels good". So §2.2's "no momentum", and the word "Momentum"
in §3's second bullet, are withdrawn, and these rules take their place.

- **A drag let go while the pointer is moving carries the rail on.** The
  pointer's speed is read over the 100 ms before the drag's last move, from
  the press on: the press's own place and time count, and so do the moves
  before the press became a drag. The rail then travels that speed times
  325 ms in all, slowing exponentially with that time constant. Only a
  drag begins a glide: a press let go short of one begins none.
- **No glide** where the pointer had rested 60 ms before it was let go,
  where its speed is under 0.1 px a millisecond, where the press ended in a
  `pointercancel` or at a move with the button up and not in a release,
  where the reader has turned animation off, or where the system asks for
  reduced motion.
- **A glide stops** at an end of the rail, at any press on the rail (a
  mouse's, a finger's or a pen's), at a wheel over the rail, when the strip
  recentres on a page turn, when the reading direction changes, where a
  tile of the rail takes focus other than focus the rail itself hands back,
  and when the strip is taken away.
- **A mouse's press that stops a glide which is still carrying the rail is
  not a click.** Its click is swallowed and the focus it took is handed
  back, as a drag's are. It can still become a drag. "Still carrying" is a
  speed of 0.1 px a millisecond or more, the speed a glide needs to begin:
  slower than that the strip looks at rest, and a press on a tile is the
  click it looks like. It stops the glide all the same. From a release at
  1 px a millisecond that is 748 ms in, where the glide itself runs on for
  about two seconds. A finger's or a pen's tap that stops a glide stays a
  tap.
- **A mouse only, as the drag is.** A finger's or a pen's press begins
  neither a drag nor a glide. What it does is stop a glide that is under
  way, so a finger's own scrolling begins on a rail that the glide has let
  go of.
- **No render.** A glide writes `scrollLeft` in animation frames and keeps
  nothing in React state. The mark follows it as it follows any scroll.

The four numbers are the prototype's, which is what the user felt. Not
everything else here was in it. The prototype asked the system's motion
setting and not the reader's own. It did not stop for a change of reading
direction, nor where `Tab` brought focus to the strip. It carried on after a
rest of exactly 60 ms. It began reading the pointer's speed where the press
became a drag, so a slow drag with a twitch at its end could be carried on.
And it took every press during a glide for one that stopped it, to the
glide's last half pixel, so that a click on a strip that looked at rest did
nothing for about two seconds after a flick.

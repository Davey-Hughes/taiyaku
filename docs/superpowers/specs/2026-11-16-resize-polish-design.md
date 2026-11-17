# Resize polish: the Continue row's fade animates, covers glide to their new columns, short screens get the phone layout, and a window keeps its place across the scroller swap (O303, O304, O305, O306)

2026-11-16. Settled by the owner in live trials on a rig served from a
scratch copy of the test library. The prototype as they tried it was built
on main 1345a786; rebased since, it is the first commit on `resize-polish`,
"Prototype as the owner tried it". Its report, with the measurements quoted
below, is `.superpowers/sdd/2026-11-16-resize-polish/glide-report.md`. This
PR opens and closes O303 to O306 (lane 2 holds O301 to O309). The prototype
was approved for feel, not for correctness: everything here is built again
under tests, and none of its trial switches ship.

## Why

- **O303.** The Continue row's fade (O288 item 3) comes and goes at once as
  the row scrolls. On a phone it is "too aggressive on such a small screen".
- **O304.** When a resize adds or drops a column, every cover jumps to its
  new place in one frame. The report measured jumps of 766 to 1032 px. The
  Continue row's cards jump with them.
- **O305.** The owner's iPhone 17 Pro Max is about 956 CSS px wide in
  landscape and about 440 px tall. That is at least 900 px, so it gets the
  desktop layout: "the sidebar and particularly bottom left of the sidebar
  takes up way too much space". Turning it sideways in iOS Safari also
  leaves a gap at the bottom until the page is scrolled, when it jumps back,
  and the page "just goes back to the top". Both come from the scroller swap
  in `shell.css`, and the second from iOS as well, as the hand-check found
  (see "Settled after the builder check", item 3). From 900 px up `main`
  scrolls inside a `height: 100%` frame; below 900 px the document
  scrolls, for iOS Safari's toolbar and status-bar tap (style-pass spec
  2026-09-17, §7.1). A turn across 900 px swaps the scroller, and the new
  one starts at 0. The `height: 100%` frame then meets iOS's toolbar,
  which leaves the gap.
- **O306.** A desktop window dragged across the breakpoint goes through the
  same swap, and its view goes back to the top.

## Goals

1. O303: each side of the Continue row fades in and out over 200 ms, and the
   fade is half as wide in the phone layout.
2. O304: when a list's column count changes, each cover on or near the
   screen glides from where it is drawn to its new place, size included. The
   Continue row glides with the covers on the same clock and curve. Nothing
   else on the page moves because of the glide.
3. O305: one definition of the phone layout, by width or by height, so a
   phone in landscape gets the phone bar, the drawer, the document scroller
   and filled covers. Every place that turns at 900 px today turns there, in
   the stylesheets and the scripts alike, and a test holds them together.
4. O306: when the scroller swaps, what was at the top of the view stays at
   the top.

## Non-goals

- Covers that fill the row at every width (`?fill`, turned down again after
  #254) and fill while dragging (`?fill-drag`). Neither ships, nor does the
  switch that chose between them.
- A glide across the edge of the phone layout. It snaps (see "The report's
  caveats").
- Fading the covers that wrap to another row instead of flying them. The
  report offered it; it was not built, and the owner tried the flight and
  picked it.
- Tablets. An iPad turning across 900 px still swaps scrollers, since it is
  tall enough for the desktop layout in landscape. O306 keeps its place. The
  toolbar gap on an iPad is not looked at here, and there is no iPad to test
  on.
- Frame times as a gate check. The report ran at load averages of 10 to 60
  on this shared machine, and its frame times sat inside that noise.
- The drawer's width. The owner, 2026-11-16: the open drawer is "way wider
  than the regular one": `min(20rem, 85vw)` (`shell.css:656`) against the
  sidebar column's `14rem` (`shell.css:22`). O305 brings the drawer to a
  phone in landscape too. This PR opens O307 for it; a follow-up PR fixes
  it.

## Anti-goals

- A glide that reads layout per cover per frame. The earlier spike did: 45
  box reads a frame at 36 covers and 549 at 540 (the report, "Reads during
  a drag with no column change"). A frame of a drag reads where each list is drawn, and no cover.
- The page shifting because of a glide: a sideways scrollbar for a frame, a
  scroll position the glide moved, or a view that comes to rest where the
  same resize without the glide would not leave it.
- A cover left with a transform or an animation, or a list left marked as
  gliding, and so still clipped sideways, after the glide.
- A React render per frame of a resize, a scroll or a glide (as in the
  2026-11-15 spec).
- A breakpoint that the scripts and the stylesheets can disagree on, or
  another script with its own copy of 900.
- A faded focus ring. O288's row: there is no fade "while a card shows a
  focus ring".
- Finding the lists by watching the whole document for changes.

## Approaches

### How a column change looks (O304)

The owner tried three ways on the rig:

1. Spare room at the row's end while dragging: rejected, "I don't like
   this".
2. Hold the gap while dragging, then ease to the spread: rejected, "feels
   bad".
3. Covers glide to their new places: picked, "this is actually better".

The glide was then tried over three layouts: `?smooth` (the Smooth spread at
rest, as on main), `?fill` and `?fill-drag`. The owner chose Smooth with the
glide: "ok I think 1 is the best". Their brief for the build: "make it as
best as you can, including adjusting if the page keeps shifting during the
animation".

The mechanism is the prototype's (the report, "How it works"). A cover's box
is arithmetic on its index, the list's width and the cover-size tokens; at a
change each cover gets a Web Animations transform from where it was drawn to
none. The Continue row does not glide by transform. Its place-keeping (O301)
reads card positions from layout, which a transform would leave stale, and a
transform would stretch its scroll range. So the row lays its column and gap
frame by frame instead.

### Where the phone layout starts (O305)

- **Picked: width or height.** The phone layout holds below 900 px wide or
  below a height H (see O305).
- Dropped: the height part only under a coarse pointer. A short desktop
  window crowds the sidebar's foot the same way, and a pointer condition
  would add a third axis to every site and every test.
- Dropped: `orientation: landscape`. Every desktop window is landscape.
- Dropped: keep `main` the scroller in landscape and fix the gap. The gap and
  the lost place both come from the swap itself, and the sidebar would still
  crowd a 440 px screen.

### One definition in the stylesheets (O305)

The build cannot share one media query today. The CSS goes through PostCSS
with one local plugin (`stripWoff`, `vite.config.ts`'s `css.postcss`), and
Lightning CSS only processes the build's output, for `build.cssTarget`. `@custom-media`
would need a new dev dependency (`pnpm-lock.yaml` has no custom-media
plugin), or the Lightning CSS transformer, which would bypass the PostCSS
step where `stripWoff` runs. The tests that read a stylesheet's `?raw` text
would also see the unexpanded name. The style-pass spec chose plain CSS and
accepted that "`@media (width < 900px)` is repeated by hand".

- **Picked:** the condition written out in full at each site, and a test
  that holds every site to one exported constant.
- Scripts that only need to know which layout is drawn go on reading
  `--tile-fill`, which `library.css` sets in its phone block "so that a
  script reads which layout is drawn rather than repeating the breakpoint".

### What O306 keeps

- **Picked: an anchor element.** The element at the top of the view is put
  back at the top.
- Dropped: the same pixel offset. The layouts differ in the covers (exact or
  filled), the phone bar and `main`'s padding, so the same offset shows other
  content.
- Dropped: the same fraction of the page, for the same reason.

## O303: the fade animates

Decided, as in the prototype:

- `--edge-before` and `--edge-after` are registered with `@property` as
  `<number>`, not inherited, initial value 1. Each mask layer is black at that
  alpha at `--fade-clear`, so 1 draws no fade on that side and 0 the full
  fade. `data-more-before` and `data-more-after` set them to 0.
- Both transition over 200 ms, `ease-out`. Under `prefers-reduced-motion:
  reduce` there is no transition.
- A focus ring in the row (`:has(:focus-visible)`) sets both to 1.
- In the phone layout the fade is half width: 40, 25 and 6 px in place of
  80, 50 and 12 ("Half width").

Settled here: the focus ring's lift takes effect at once, and only the
fade's return after the ring goes is animated. In the prototype the
transition runs both ways, so a ring at the row's edge is partly faded for
up to 200 ms, against O288. A rule for the row with a ring that sets no
transition does this: a transition follows the style changed to, so the
change into that rule is immediate and the base rule's 200 ms carries the
change back.

Left to the builder: the comment above the fade rules now sits over the
`@property` rules (`library.css:154` in the prototype); it moves
to the rule it describes.

## O304: covers glide when a list's column count changes

The lists:

- The home grid and the search hits: `SeriesGrid.tsx`'s `<ul className="grid">`,
  drawn twice by `Library.tsx` (the grid and the search hits).
- A series page's volumes: `Series.tsx`'s `<ul className="volumes">`, in the
  grid layout only. The table never glides.
- The Continue row (`useContinueRow`).

What glides, decided:

- A change in a list's column count with the same covers: a window resize,
  or a Cover size slider step that adds or drops a column. A change in the
  number of covers (a chip, a search, data arriving), alone or in the same
  frame as a column change, snaps.
- Each cover moves from where it is drawn at that frame, mid-glide included,
  to its new box, size included. The transform runs from the translation and
  scale that put it where it was, to none, scaled about its top-left corner
  so the image is never distorted. It is relative to the cover's live box, so
  a page still moving under it carries the cover, and the glide ends on the
  box. A second change mid-glide starts the next glide from where the cover
  is drawn then.
- The tuned values: 180 ms; `cubic-bezier(0.25, 1, 0.5, 1)`; a cover that
  starts or ends within 200 px of the viewport glides and one beyond snaps; a
  move under 2 px, in position and in width, snaps; under reduced motion
  nothing glides and the Continue row lays its target at once.
- The Continue row is laid frame by frame on the covers' clock and curve,
  from the column and gap it drew at the change to the live target, and keeps
  its place each frame the way a resize does (O301).
- While any cover in a list glides, the list clips sideways
  (`overflow-x: clip`), so a cover drawn where it stood a frame ago cannot
  give the page a sideways scrollbar.
- One box read per change checks the arithmetic against the page. If it is
  off by more than 1.5 px, every cover in that list snaps.
- A change that turns the phone layout on or off snaps, by either edge,
  whether or not the column count changes. For example, 240 px covers at
  1280 px wide lay 4 columns in both layouts by the tokens' arithmetic (not
  measured), so a window crossing the height edge there changes the layout
  and not the count.

The page holds still. This is the owner's "adjusting if the page keeps
shifting":

- In Smooth a Continue card keeps its width at a column change, so the row's
  height holds and nothing below it moves.
- In the phone layout the covers fill their columns, so the row's height
  follows its column width through the glide. The browser's scroll anchoring
  then corrects the page every frame instead of once. The report found the
  page coming to rest 15 px (Chromium) and 42 px (Firefox) from where the
  same resize leaves it without the glide, scrolled down. The PR closes this:
  the page rests where a resize without the glide leaves it. The route: the
  row glides only where a cover would, on screen or within the 200 px
  margin, and otherwise lays its target at once, which leaves the browser
  one correction to make. In the phone layout it is narrower than first
  recommended: the row is laid at once wherever its covers lie wholly above
  the view, their foot worked out from the row's box and column rather than
  read. Chromium's corrections frame by frame add up to the one, but
  Firefox's need not: with the row just above the view, a column change made
  and unmade turns Firefox's scroll anchoring off and leaves the page off.
  Where a cover is partly in view the browser anchors to it, so the row
  slides there as before; laid at once, it would push the grid under covers
  just starting their glide. A card's placeholder text is kept out of the
  browser's choice of anchor, so a placeholder across the view's top
  anchors by its box, as a cover does. The narrowing is put to the owner
  with the PR.
- A glide moves each tile's content (its first child: a volume's
  `.tile-body`, a series' link), never the tile. Chromium makes no
  scroll-anchoring correction while the element it anchors to, or one
  around it, runs a changing transform, so with the tile moving, a column
  change mid-glide left a scrolled page a row from where a resize without
  the glide leaves it. The content is kept out of the browser's choice of
  anchor at all times (`overflow-anchor: none`): kept out only while a list
  glides, Chromium keeps an anchor it chose inside the content before, and
  a column change mid-glide can go uncorrected in its frame. The trade, put
  to the owner with the PR: at rest the browser holds a tile's top, not the
  caption at the view's top, so where a cover's height changes (a resize in
  the phone layout, where covers fill their columns; a Cover size step) a
  page scrolled to a caption rests by that change from where main leaves
  it, with or without the glide.
- An open volume panel (O283) on a series page stays above the covers and
  is laid from its tile's box. The panel is outside the moving content, so
  its tile has no transform and is no stacking context, the panel's
  `z-index: 5` holds against the covers after it, and `tilePanel.ts`'s
  `layPanel`, called from a `ResizeObserver` in `Series.tsx` that fires
  through a drag, reads the tile's laid box. The panel stands at that box
  while its cover glides to it. (Found by reading; the report's series-page
  check had no panel open.)

How it attaches, decided:

- From the components, by a hook with a ref, in the shape of
  `useContinueRow`: on `SeriesGrid`'s `.grid` and on `Series.tsx`'s
  `.volumes`. One shared `ResizeObserver` and the window listeners start
  with the first list and stop with the last.
- Nothing at module level needs a browser. jsdom has no `matchMedia`, no
  `ResizeObserver` and no `Element.animate` (`Shell.tsx` and `tileSize.ts`
  guard `matchMedia` for that reason), so the hook does nothing there, and
  importing any of these modules in the jsdom project must not throw.

Left to the builder:

- The names of the hook and the modules. The reader's strip already uses
  "glide" for its own motion (`Filmstrip.tsx`, `tiles.ts`, O286), so pick
  names a grep for one will not find in the other.
- How a list learns that the phone layout turned: one `MediaQueryList` built
  from the shared condition, or `--tile-fill` read when the list's width
  changes.
- The route to a page that holds still in the phone layout, above.
- Whether the self-check's snap also writes to the console, as the
  prototype's does. `MetadataEditor.tsx` logs an impossible state the same
  way.

## O305: the phone layout by width or height

Decided: short landscape screens get the phone layout. The condition, with
the recommended H of 500 px:

```css
@media (width < 900px) or (height < 500px)
```

and its complement, for the rules that hold only where the sidebar shows:

```css
@media (width >= 900px) and (height >= 500px)
```

The builder confirms H or changes it, and records the reasoning and the
device figures used in the notes:

- Every phone in landscape must be under H with its browser bars shown or
  hidden, so the bars never flip the layout. The owner's phone is 440 px
  tall in landscape.
- iPads, and tablets whose short side is about 700 CSS px or more, stay
  over H in landscape. The style-pass spec (§2, "Breakpoint") gives "a
  landscape iPad and a desktop the sidebar". A tablet with a 600 dp short
  side gets the phone layout in landscape, for the same reason a short
  desktop window does; this is put to the owner with the PR.
- A desktop window shorter than H gets the phone layout. At that height the
  sidebar's foot crowds the window the way it crowds the phone.
- No soft keyboard may push a device across H. `index.html`'s viewport sets
  no `interactive-widget`; the builder checks which browsers resize the
  layout viewport for a keyboard. Should one turn the layout while someone
  types, the search field is the same element in both layouts
  (`SearchBar.tsx`, in `main`), so it keeps its focus and its text; a test
  holds that.

On a phone in landscape this gives the phone bar and the drawer, the
document scrolls in both orientations (so a turn no longer swaps scrollers
and the gap goes; the jump to the top also came from iOS, and O306's keeper
answers it, see "Settled after the builder check", item 3), the covers fill
their columns
(O285's "Fill on phones"), the Continue heading is drawn as on phones (O288
item 1 holds only beside the sidebar), the fade is half width, and the
reader's strip tiles are 96 px (the owner took it; see "Settled after the
builder check").

Every site that turns at 900 px, with lines as on main 1345a786. Each takes
the condition or its complement.

Phone side, `@media (width < 900px)`:

- `web/src/styles/shell.css:567`: the phone bar, the drawer, the document
  scrolls.
- `web/src/styles/library.css:611`: the room under a panel holds the bottom
  safe-area inset.
- `web/src/styles/library.css:635`: the document keeps its scrollbar gutter
  on a series page.
- `web/src/styles/library.css:647`: `--tile-fill: 1` and the filled columns
  (O285, O302).
- `web/src/styles/library.css:861`: the search box's input takes the row.
- `web/src/styles/reader.css:109`: the strip's tiles at 96 px.
- `web/src/styles/settings.css:696`: the settings dialog's top.
- `web/src/styles/settings.css:1038`: the document's gutter on Configuration
  screens.
- The prototype's half-width fade (`library.css:211` on `resize-polish`).

Desktop side, `@media (width >= 900px)`:

- `web/src/styles/library.css:91`: the Continue heading level with the
  sidebar's Libraries heading (O288 item 1).
- `web/src/styles/admin.css:864`: Rules side by side (its container query).

Scripts:

- `web/src/Shell.tsx:56`, `WIDE`, used at :207: the drawer closes when the
  window leaves the phone layout.
- `web/src/tileSize.ts:37`, `STRIP_BREAKPOINT_PX`, used at :111: the strip's
  rung, read once per mount.
- `web/src/library.ts:224`, `coverSizes`: a cover image's `sizes`.
- The prototype's `isNarrow` (`glideTiming.ts:59`), read at `glide.ts:154`
  and `continueRow.ts:164`. It goes: see "From the prototype to the PR".

These read the layout from the stylesheet and follow without a change:
`continueRow.ts:129` reads `--tile-fill`, and `scrollKeeper.ts`'s
`scroller()` (:58) reads `main`'s `overflow-y`.

Not this breakpoint, and left alone: `book.css:237` `(max-width: 40rem)` and
`admin.css:1891` `(width < 36rem)`.

How the stylesheets and the scripts agree:

- One TypeScript module exports the condition and its complement. `Shell.tsx`,
  `tileSize.ts` and `coverSizes` import them; `STRIP_BREAKPOINT_PX` and
  `WIDE` go.
- `coverSizes` writes the condition as two entries, `(width < 900px)` and
  `(height < 500px)`, each with the filled width, before the bare size. An
  image's `sizes` then needs range syntax, which it already relies on, and
  not `or`, which the owner's Safari cannot be checked for here.
- A node test reads `src/index.css` and every stylesheet under `src/styles/`.
  Every `@media` that tests the viewport's width or height is the condition
  or its complement, letter for letter, except a named list (the two above).
  The existing ties read the exported constant instead of a literal:
  `library.css.test.ts:30`, `:41` and `:75`, `settings.css.test.ts:34` and
  `:42`, `reader.css.test.ts:89` (O148's pin), and `library.test.ts:359` to
  `:360`.
- `builtStylesheet.test.ts` holds that the built stylesheet still has the
  condition and its complement after Lightning CSS.
- A browser test holds the layout at sizes either side of both edges (899
  and 900 px wide at a tall height, 499 and 500 px tall at a wide width,
  956×440, 440×956 and 1280×800): the condition's `matchMedia`, `--tile-fill`,
  the phone bar being drawn, which element scrolls and the drawer closing
  all agree.

`Shell.browser.test.tsx:1381` drives the account screen at 1280×300 and
expects the desktop layout ("leaves the document unscrolled on the account
screen in a short window"). At 1280 px the account row lies less than H
deep, so no window at or above H is shorter than it. The case runs at a
height of H and lowers the row instead, padding `main`'s top
(`tests/support/shortWindow.ts`'s `lowered`) until the row lies deeper than
the window, which the case already checks; it checks too that the sidebar's
layout holds. `Libraries.shell.browser.test.tsx` shrinks a 1280 px window
the same way in two places, `atTheFoot` (:667, to `below - by`) and the
case at :1146 (to `top + 10`). Either can drop under H and turn the phone
layout on mid-test, so each lowers the page the same way where the
sidebar's layout holds before the window shrinks, and checks that the
window keeps its layout. The builder greps the browser tests for every
other `viewport(` call that can go under H.

## O306: the place is kept across the scroller swap

Decided:

- When the phone layout turns on or off and the scroller swaps (`main` to
  the document, or back), the element at the top of what is seen stays at
  the top, within 1 px, in the first frame drawn in the new layout. In the
  phone layout what is seen starts under the phone bar. Inside a cover list
  the anchor is the first cover on the top row seen; elsewhere it is the
  element at the top edge. A page at its top stays at its top. A page that
  cannot scroll that far rests at its end.
- This holds on every page in the shell: the home grid, search hits, a
  series page and the Configuration screens. One owner does it for all of
  them, `Shell.tsx` or the scroll keeper.
- Back after a swap lands where the place was put back. The scroll keeper
  saves a scroll as each scroll event fires (`scrollKeeper.ts`'s `scrolled`),
  so the 0 the swap passes through must not end up as the entry's kept
  scroll.
- Crossing back and forth ten times keeps the same anchor at the top.

Left to the builder: how the anchor is chosen and when it is recorded. By
the time a swap is known the old layout is gone: a read in a frame that has
resized lays the page out at the new size (the prototype's `resizing` flag
in `glide.ts` exists for the same reason). So the anchor is recorded ahead,
after layout (a `ResizeObserver` callback, or a scroll seen at the next
frame), and put back before the first paint in the new layout.

## The report's caveats, settled

- **Crossing the breakpoint snaps.** Kept, for both edges of the phone
  layout. On a phone a turn no longer crosses it; a turn snaps on its own
  check (see "Settled after the builder check").
- **Covers that wrap fly diagonally, up to about 1000 px in 0.18 s.** Kept:
  the owner tried it and picked it. Fading them instead is not built.
- **A focus ring at a list's left or right edge is clipped for the glide's
  0.18 s.** Accepted. Without the clip the page would flash a sideways
  scrollbar. Nothing is clipped once the glide is over; the end check holds
  that the list is no longer marked as gliding.
- **Titles scale with their cover during a size glide.** Moot from the
  desktop layout up, where Smooth covers keep their size. Not moot in the
  phone layout, whose covers fill their columns: a slider step or a narrow
  window that changes the column count scales a title by up to about 20 %
  for 0.18 s, and its line breaks change at the change. Accepted as part of
  the option the owner picked; `?smooth` glided this way below 900 px.
- **The page resting 15 or 42 px off after a scrolled Fill glide.** Moot for
  `main`, which never fills now. Live in the phone layout, where the
  document scrolls; settled by O304's "The page holds still".
- **Firefox switching scroll anchoring off for `main` after a Fill drag.**
  Moot, since `main` never fills now; it happened with or without the glide.
- **`?fill`'s image sizes.** Moot: a trial edit, removed.
- **The Continue row pausing for one frame at a change mid-glide.**
  Accepted: under one frame of motion.
- **No Safari on this machine.** The owner's hand-check (see "Testing").
- **The body `MutationObserver`.** Replaced (see the next section).

## From the prototype to the PR

Removed, as trial-only:

- `web/src/trialLayout.ts` and its import in `main.tsx`.
- The trial rules at the end of `library.css`'s cover layout
  (`data-trial-layout`, `data-trial-resizing`).
- `coverSizes`'s `?fill` branch in `library.ts`.
- `?noglide`: `off` and `glideOff` in `glideTiming.ts`, and the
  `if (!glideOff())` around `glide.ts`'s setup.
- `LAYOUT_EVENT`, `glide.ts`'s `epoch` and `continueRow.ts`'s `turned`
  listener. Only the trial turned the layout without a resize. A real turn
  always changes the lists' width, since the sidebar's column and `main`'s
  padding change with it.
- `glide.ts`'s import in `main.tsx`, its body `MutationObserver` and the
  `querySelectorAll` that ran on every change to the document.
- "Prototype" and "trial" in comments.

Fixed:

- `glideTiming.ts` calls `window.matchMedia` at import (:48 and :59), and
  jsdom has none. `continueRow.hook.test.tsx` and every jsdom test that
  renders `Library` import `continueRow.ts`, which imports it, so they would
  throw at import (read, not run). Likewise `glide.ts`'s module-level
  `new ResizeObserver` (:371) once a component imports it.
- `isNarrow` is a fourth copy of the breakpoint. `Laid.fill` and
  `Laid.narrow` say the same thing once the trial is gone; one of them is
  enough.
- `onRowLaid` keeps a single callback, and a second caller would replace the
  first. The Continue row tells every watched list instead.
- The fade transitions into the focus state (see O303).
- The open panel during a glide (see O304).
- `glide.ts` reads every watched list's box on each scroll of the page (a
  capturing `scroll` listener), whether or not anything resizes. Kept only if
  no cheaper way keeps the start error under the check's bound, and a test
  bounds the reads per scroll.

Kept: `GLIDE_MS`, `GLIDE_EASING` and `eased`, with a test that the two
curves agree; the start time set to the frame's time; the per-change
self-check; `transform-origin: 0 0` on each tile's content; the sideways
clip while a list glides.

## Constraints

- No React render per frame of a resize, a scroll or a glide.
- A frame's reads do not grow with the number of covers.
- Each new test is watched failing with the defect it names applied by hand,
  and is driven with real input: real resizes (`page.viewport`, a step a
  frame, as `Library.continue.browser.test.tsx`'s window drags), a real
  mouse, real keys. Reduced motion is emulated as `Series.tile.browser.test.tsx`
  does it, through `Emulation.setEmulatedMedia`. The browser project is
  Chromium only and CI does not run it; the local gate does.
- Firefox is the owner's browser (2026-11-14 spec, "Approaches") and is not
  in the browser project. The builder runs the start, end and
  still-page checks once in Firefox, as the report did, and records the
  figures in the notes.
- A comment says why. The 900 px in comments (`grep -n 900` across `web/src`)
  becomes "the phone layout" where the comment means it.

## Testing

The reference for "without the glide" is the same drive under reduced
motion, which takes the no-glide path.

O303, in `Library.continue.browser.test.tsx`:

- The fade animates: about 100 ms after a side starts or stops fading its
  alpha is strictly between 0 and 1, and at 200 ms and a frame it is at its
  end; the transition is 200 ms `ease-out` with no delay. Defect: the
  `@property` rules removed (the value then flips at once).
- A focus ring, reached by a real Tab, takes both sides to alpha 1 in the
  first frame, and the fade comes back over 200 ms once focus leaves.
  Defect: the transition left running into the focus state.
- Under reduced motion the transition is `0s` and the alpha flips in the
  next frame. Defect: the reduced-motion block removed.
- The stops are 6, 25 and 40 px at 390×844 and at 956×440, and 12, 50 and
  80 px at 1280×800. Defect: the half-width block written width-only (956×440
  fails).
- The existing O288 fade tests are restated: the mask now always has both
  layers, a side that does not fade reads alpha 1 at every stop, and "lifts
  the fade while a card has a focus ring" expects alpha 1, not `none`.

O304, in the browser project, on the home grid and on a series page:

- Starts where drawn: in the frame a glide starts, every cover on screen is
  drawn where it was drawn the frame before, within 0.5 px, for one column
  each way, a reversal 3 frames into a glide, and a second change 4 frames
  in. Defect: the animation's start time left to the browser, or the old box
  taken without the running glide.
- Ends clean: the glide's length and two frames after the last change, no
  cover has an animation, every computed transform is `none`, no list is
  marked as gliding, and every box is where the reduced-motion drive puts it,
  within 0.5 px. Also after a chip filter changes the list mid-glide.
  Defect: a final keyframe other than none, or an animation filled forwards.
- Reads bounded: box and style reads, counted per frame with spies over a
  60-frame drag with no column change, are the same at the fixture's covers
  and at about 500. Defect: each cover read every frame.
- Snaps: no animation across the phone layout's edge by width, and by height
  where the column count holds (the 240 px case above, measured first); none
  for a cover beyond the 200 px margin, none under reduced motion. Defect:
  each condition removed in turn.
- In step: through an uninterrupted glide, with the Continue row at its
  start, Continue card k's left edge is within 0.5 px of the grid's first-row
  cover k at every frame. Defect: `GLIDE_EASING` changed without `eased`.
- Holds still: in no frame of a glide do `main` or the document scroll
  sideways; in Smooth, scrolled down in `main`, the scroll position matches
  the reduced-motion drive's at every frame; in the phone layout, scrolled
  down in the document, the page rests within 1 px of where the
  reduced-motion drive rests. Defects: the sideways clip removed; the row
  gliding while far off screen.
- The open panel: with a volume's panel open, mid-glide the point at the
  panel's centre hits the panel (`elementFromPoint`), and after the glide the
  panel is laid where the reduced-motion drive lays it. Defects: the
  transform put back on the tile that holds the panel; `layPanel` reading
  the moving content's box.
- The slider: an arrow key on the Cover size slider that adds a column
  glides, and ends clean.
- jsdom: the existing jsdom tests that render `Library` and `Series` are the
  check that nothing needs a browser at import. Defect: a module-level
  `matchMedia` call.

O305: the node and browser agreement tests above, and at 956×440:

- A real click on ☰ opens the drawer and focus moves to the first link; the
  document scrolls and `main` does not; the covers fill; the Continue
  heading has its phone margin; a cover asks for the filled width's image,
  as `Cover.browser.test.tsx` checks at 390 px. Defect: any one site written
  width-only.
- The drawer, open at 956×440, closes when the window grows to 956×800.
  Defect: `Shell.tsx`'s query left width-only.
- The search field keeps focus and its text through a layout turn by
  height.

O306, at 1280×800, the home grid scrolled to its middle:

- A width drag to 880 px, a step a frame, and back; and a height drag to
  480 px and back. On each side the anchor is at the top within 1 px in the
  first frame of the new layout, and after ten crossings it is still the
  same anchor. The same on a series page and a Configuration screen; a page
  at its top stays there. Defect: the restore removed (the view lands at 0).
- Back after a swap lands at the kept place. Defect: a place kept from the
  page left put back over Back's landing.

Hand-checked by the owner on the iPhone 17 Pro Max over the tailnet, since
Safari cannot run here (read the user-agent first):

- Landscape draws the phone bar and the drawer; no gap at the bottom after a
  turn; a turn keeps the place, and a turn and its return land where the
  reader started ("Settled after the builder check", item 3).
- The fade animates and is half width in both orientations.
- A Cover size slider step glides in both orientations.
- A cover in landscape is sharp (the server log's `?w=` for its image).
- A turn on each of the two builds, snap and glide, to pick one; and
  whether the page keeps its place after a turn (both under "Settled after
  the builder check").
- On the snap build a turn moves no cover and no card: they appear in place
  at once. Any motion there is a defect to report, not a preference: the
  turn's signal did not reach the lists in Safari.
- A slider step on a page scrolled a little, until the Continue row
  straddles the top, with iOS's Reduce Motion on: the covers and the row
  move at once, and where the page rests is the reference for the next line.
- The same step from the same scroll with Reduce Motion off leaves the page
  where Reduce Motion left it.
- Whether Safari reads `or` inside a cover's `sizes`, which `coverSizes`
  does without by naming each part of the condition: at the smallest Cover
  size in landscape, a build whose `coverSizes` writes the condition with
  `or` asks for the same `?w=` in the server log as this one.

## Docs

- O303 to O306 are new rows, opened and closed in this PR, each by the
  five-part edit. O307 (the drawer's width, see Non-goals) and O308 (a kept
  scroll restored into another layout, see "Settled after the builder
  check") are opened and left open.
- `docs/notes.md` gets a section for them: the height H and its reasoning,
  the Firefox figures, and each defect a test was watched failing against.
- A dated line, as O302 added to the 2026-11-14 spec's item 9: in the
  style-pass spec's §2 ("Breakpoint") and §7.1, and in the 2026-11-14
  spec's item 9 and Non-goals, which say "900 px" for the phone layout. The
  closed rows (O285, O288 and the rest) are history and are not edited.

## Settled after the builder check

1. **A phone turn: snap, decided in the hand-check.** Under O305 a turn stays
   in the phone layout, so it is a column change inside one layout, and the
   covers and the Continue row would glide. The owner had not seen this: in
   the prototype a turn crossed 900 px and snapped. The owner, 2026-11-16:
   decide it in the hand-check. So the PR snaps on a turn (a change in the
   screen's orientation) and glides on everything else, and whether a turn
   glides is decided in one place, a single constant. The hand-check rig
   serves two builds on two tailnet ports: the PR as it stands, and the same
   tree with that constant flipped. Neither build carries a switch a viewer
   can reach. The owner tries both on the 17 Pro Max; if they pick the glide,
   the constant flips and the turn's snap goes, with its test. The builder
   chooses the orientation signal (`screen.orientation`'s `change`, an
   `(orientation: …)` `MediaQueryList`, or the two dimensions swapping) and
   orders it against the `ResizeObserver` callback. A desktop window
   dragged across square also turns the orientation and snaps once, which
   is accepted. The test emulates the turn through CDP
   (`Emulation.setDeviceMetricsOverride` with `screenOrientation`) and is
   watched failing with the turn's check removed. The owner, 2026-11-17,
   tried both: "they don't really seem different on iOS Safari", so the
   snap stays.
2. **The reader's strip on a short landscape screen: 96 px.** The owner,
   2026-11-16: take it. The reader shares the condition, so `reader.css:109`
   and `tileSize.ts` take the shared condition like every other site.
3. **Where the page lands after a phone turn: O306's keeper puts it back,
   in this PR.** With no swap, a turn still changes the column count and
   the page's height with it. Chromium and Firefox keep what is on screen
   by scroll anchoring. The owner's iPhone did not keep it: iOS Safari set
   the page's scroll itself by the turn's resize, and from a sliver of the
   Continue row near the home's top a turn went to the top. The owner,
   2026-11-17: fix the turn in this PR, and a turn and its return land
   where the reader started, so turning back and forth does not drift. So
   O306's keeper puts the place back at a turn as at a swap, and reuses the
   place it put back until a scroll moves the anchor or the Cover size
   steps. Whether iOS Safari anchors through a turn is not settled; the
   put-back lands the same either way. On the iPhone with the keeper, the
   home turned from its middle and from the sliver both held.
4. **A kept scroll restored into another layout: out of scope (a call made
   for the owner to check).** The scroll keeper stores a pixel offset per
   history entry, and hands the reader's way out a pixel offset for the last
   Library view. After the layout changed elsewhere (a phone turned while
   reading, a window resized on another page), Back or leaving the reader
   lands at that offset, which shows other covers. This is how it works on
   main for any resize, and making the store anchor-based is a task the size
   of this PR. This PR opens O308 for it and leaves it open.

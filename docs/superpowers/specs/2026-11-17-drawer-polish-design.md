# Drawer polish: the drawer is the sidebar's width, its foot scrolls on a short screen, and the phone bar moves with the scroll (O307, O309, O337)

2026-11-17. Settled by the owner on their iPhone 17 Pro Max, against a
prototype served over the tailnet from a scratch copy of the test library.
The prototype is three throwaway commits on `drawer-polish`, each named
"Prototype (throwaway): …"; its rig, measurements and screenshots are in
`.superpowers/sdd/2026-11-17-drawer-polish/` (`rig-report.md`,
`check-measurements.json`, `shots/`), and the owner's trials are in that
directory's `rig-requests.log`. The owner approved the prototype for feel,
not for correctness: this PR builds the chosen parts again under tests, and
none of the prototype's switches, its `proto*` files or its logging ship.

## Why

All three come from the resize polish (#259), which gave a phone held
sideways the phone layout: the phone bar, the drawer and the document
scroller.

- **O307.** The open drawer is `min(20rem, 85vw)` wide, against the `14rem`
  of the sidebar column it stands for. The owner, 2026-11-16: "way wider
  than the regular one".
- **O309.** On a phone held sideways the drawer is the window's height,
  about 440 CSS px. Its foot (the build's label, More, and the Theme switch
  with its heading) is sticky at the bottom, and with 44 px rows for a
  finger it took about 220 px of the 440, measured headless at 956×440
  (`check-measurements.json`). The navigation scrolls in what is left. The
  owner: the foot takes most of the drawer, and "this probably need[s] to be
  adjusted for small screens somehow".
- **O337.** As an iPhone turns, the phone bar disappears and pops back. The
  owner tried four bar positions on the phone: `sticky` (today), `sticky`
  with `will-change: transform`, `fixed`, and `static`. Every pinned one
  pops when the turn starts mid-page, and none pops at the very top. The
  likely cause is iOS drawing a layer pinned to the viewport again once the
  turn is over, though on apple.com and github.com the owner found that
  the menu bar "basically doesn't pop back up after the turn"
  (2026-11-17). A bar that is part of the page turns with the page.

While trying the bar, the owner asked for a different behaviour: the bar
goes away as you scroll down and comes back as you scroll up, and it moves
with the scroll "like iOS does with its menu elements". That behaviour also
answers most of O337. Mid-page the bar is usually out of view, and when it
rests in view it can be part of the page.

## Goals

1. The open drawer is the sidebar column's width, `14rem`, plus the left
   safe area it pads inside itself.
2. On a screen under 500 px tall, the drawer's foot sits after the
   navigation and scrolls with it, so the navigation has the whole drawer.
3. In the phone layout the phone bar moves with the page. Scrolling down
   carries it out of view, pixel for pixel. Scrolling up brings it back the
   same way, and once it is fully in view it stays pinned at the top.
4. When scrolling stops with any part of the bar in view, the bar becomes
   part of the page at the same pixels, so a turn draws it with the page.
   The next scroll pins it again.
5. A turn neither shows nor hides the bar: the part of it in view before
   the turn is in view after it.

## Non-goals

- The sidebar beside the page (900 px wide and 500 px tall and up) is
  unchanged, foot included.
- A phone held upright keeps today's foot. It is over 500 px tall, so the
  foot stays sticky.
- The reader's own top bar (style-pass spec §8.3) is unchanged.
- O308 (a kept scroll restored by anchor) stays open and out of scope.
- The tailing "very slight pop" (below) is not chased past one cheap
  attempt.

## Anti-goals

These would make the PR a failure even if every test passed.

- **The bar lags the page while it moves.** A transform written from a
  scroll listener runs on the main thread, and iOS scrolls on another, so
  the bar would trail the content. The bar's motion must come from layout
  that the compositor moves with the page.
- **A turn shows or hides the bar.** iOS sets its own scroll position as
  the window resizes. That scroll must not read as the reader scrolling up
  or down.
- **The bar covers the page's first row at the top.** At scroll 0 the bar
  is fully shown and the content starts below it, as today.
- **The bar jumps while it changes mode.** Every switch between pinned and
  part of the page happens at the same pixels.
- **A short drawer hides More's menu.** Under the scrolling foot, More's
  panel still opens whole, or scrolls inside itself as it does today when
  the room above it is short.

## Approaches

The owner chose each part by trying it on the phone.

**The foot on a short screen.** Three were served:

- **A, the foot scrolls with the list.** Chosen. Nothing moves or goes, and
  the list gets the whole drawer.
- **B, one row:** an icon-only Theme switch beside More, with the build's
  label moved into More's menu. It does not fit at 14rem: More with its
  word, the open-proposals count and its chevron is 119 px wide, and beside
  three 44 px segments it got 47. The prototype had to shrink the icons to
  36 px and drop More's chevron.
- **C, the foot is just More,** with Theme and the label moved into More's
  menu. Theme is a tap deeper. For a non-administrator More is a link, so
  on a short desktop window it would take the bottom-left corner that the
  owner's ruling of 2026-10-27 gave to Theme, since a browser draws a
  hovered link's address there.

**The bar.** Tried in this order:

- **The bar in four positions** (above). All the pinned ones pop.
- **Snap:** the bar slides out (200 ms) after a scroll down and slides back
  after a scroll up of 24 px. The owner: it "feels ok", but asked for the
  bar to track the scroll.
- **Track,** chosen: the bar moves with the scroll, as described below.
- **At rest: pinned or in the page.** Pinned at rest, the bar stays pinned
  to the viewport like the positions that popped. In the page, chosen, the
  owner: the pop is "mostly" gone, "a very slight pop now but it's better".

## O307: the drawer's width

In the phone layout's block of `shell.css`, `.sidebar-column` is
`width: calc(14rem + env(safe-area-inset-left, 0px))`. Its left padding is
already that inset, so the column's content keeps the 14rem it has beside
the page.

The left inset on the owner's phone held sideways is unmeasured: the
prototype logged insets only on `/proto.html`, which the owner opened
upright. Note, too, that an earlier hand-check found Safari draws no page
over the camera cutout. The formula is right either way: with no inset the
drawer is 14rem.

## O309: the foot scrolls with the list on a short screen

Under `@media (height < 500px)` (inside or beside the phone layout's block;
a height under 500 px is always the phone layout), `.sidebar-foot` is
`position: static`. The column already scrolls (`overflow-y: auto`) and the
foot already follows the navigation in the flow, so the foot now sits after
the last library and scrolls into view at the end of the list. Its contents,
order and look are unchanged, and so is everything a non-administrator sees.

The drawer's Close button and wordmark are in the same scrolling column and
already scroll away when the list is long. That does not change.

More's panel opens upward and measures the room above More
(`AccountMenu.tsx`, `upToClip` and `--room-above`). With the foot scrolled
into view at the end of the drawer, that room is the column's visible
height above More. The build checks that the panel opens whole there, or
scrolls inside itself when the room is short, as it does today.

## O337: the phone bar moves with the scroll

### The track

The bar stays `position: sticky; top: 0`, but its parent is a new element,
the **track**: a box at the page's top, the page's width, and `E` px tall,
inside a **clip**, `position: absolute` over the whole page (`.library`,
positioned) with `pointer-events: none` (the bar sets `auto`). A sticky element
cannot leave its containing block. So while the scroll position `y` is at
most `E − H`, the bar is pinned at the top. Once `y` passes `E − H`, the
track's bottom pushes the bar up with the content, and the compositor moves
it in step with the page. `H` is the bar's height, `--sticky-top` (52 px
plus the top safe area).

The content is padded down by `H` so it starts below the bar, as it does
today with the bar in the flow.

Script moves only `E`. On every scroll event, with `y` clamped to
`[0, maxScroll]` so that iOS's rubber band counts for nothing:

```
E = max(H, clamp(E, y, y + H))
```

Scrolling down from pinned, `E` holds while the bar slides out. Once the
bar is out of view, `E` follows `y` and keeps the bar just above the top
edge. Scrolling up, `E` holds while the bar slides in from above. Once the
bar is fully in, `E` follows `y + H` and keeps it pinned. `E` changes
only when the bar is fully in or fully out of view, so a late scroll event
changes nothing on screen. The `max(H, …)` keeps the bar fully shown at
scroll 0.

`maxScroll` is the page's own. Without the clip, the track and the bar at
rest in it would be part of the page's height, so a page shorter than the
track's end, one grown shorter in place or one a navigation or a turn
brings, would be held up by them, blank under the bar, and a shrink in
place would show that for a frame however soon the script heard of it:
the browser draws the clamped page before the clamp's scroll event. The
clip is `overflow: clip`, so whatever of the track or a resting bar runs
past the page's end is cut off and adds nothing to the page's height, and
the browser clamps the scroll to the page's own end, as it would with no
track. `clip`, unlike `hidden`, makes no scroll container, so the bar
inside still sticks to the window.

`E` is written to the DOM from a passive scroll listener, not through React
state. A render per scroll frame is the defect O216's prototype had.

**The bar is forced fully in view** (`E = y + H`) while the drawer is open
and while keyboard focus (`:focus-visible`) is inside the bar. Focus that
is not keyboard focus does not hold it. The drawer hands focus back to ☰
on every close, and a library tapped in the drawer closes it, so a touch
reader leaves focus on ☰ most of the time; if that held the bar, the bar
would never scroll away after the drawer had been used. This is my call
(builder check, item 1).

**A navigation shows the bar.** On a change of path or query the bar comes
fully into view (`E = y + H`), whether the new page opens at the top or at
a kept place, as Safari shows its own toolbars on a new page. A jump to a
fragment is a scroll, and a replace that keeps the path (a search
keystroke, a chip, the sort or an edition) refines the page shown; neither
is a navigation for the bar. From there the rule above applies. Other
scrolls that the reader did not make, such as scroll anchoring's
corrections or iOS scrolling a focused field into view, move the bar by
the rule like any scroll. Telling them apart from the reader's
own scrolls would be a different design, and they are small. This is my
call (builder check, item 2).

**Under `prefers-reduced-motion`** the bar still tracks. The reader's own
scroll moves it, and nothing animates by itself. This is my call.

**Leaving the phone layout:** the bar is not drawn there (today's CSS), and
the track's script stops. On coming back, the bar starts fully shown.

### At rest, the bar is part of the page

When scrolling stops with any part of the bar in view, the bar becomes
`position: relative` at its own document offset: the same pixels, now part
of the page. It stays in the track's flow, as the sticky bar is, so the
page's scroll anchoring is not suppressed after it rests. "Stops" is
`scrollend`, which Safari on the owner's phone fires
(the log's rest lines give `why=scrollend`). Where `scrollend` is missing,
150 ms with no scroll event counts instead.

The bar rests in the page whenever any of it is in view and the page is not
scrolling, not only after a scroll. That covers the drawer closing, a
navigation, and a turn's end. Otherwise a bar left pinned by one of those
pops at the next turn. This is my call (builder check).

The next `wheel`, `touchmove` or `scroll` event puts the bar back on the
track. A `wheel` or `touchmove` that scrolls only something inside the
page, such as a sideways swipe on the Continue row, makes no page scroll
and so no page `scrollend`. If no page scroll follows within 150 ms, the
bar rests again. This is my call (builder check). On a scroll up, `E` is set so that it stays pinned. On a scroll down
it rejoins the track where it is, and the track carries it out. The
prototype did this on `wheel` and `touchmove` as well as `scroll`, so the
switch lands before the first scrolled frame. In the owner's trials this
showed no jiggle.

A page that shrinks in place has its scroll clamped in layout, and the
clamp's `scroll` event comes a frame later, after the clamped page is
drawn: a resting bar would be drawn for that frame where the page moved
from, lower in the window or, past the page's new end, cut off by the
clip. A `ResizeObserver` on the clip, which resizes with the page, runs
the scroll's handling once the clamp has moved the scroll, before the
frame is painted, so the bar is back on the track and where the rule puts
it in the frame that draws the shrink (through a turn, the held share is
re-placed instead). Two shrinks it hears late or not at all are known
limits (the notes, "O307, O309 and O337"): one made inside another
`ResizeObserver`'s callback with no window resize, a frame late, and the
shrink of an overhang past the page's end, which does not resize the clip.

### A turn

A turn is a resize that changes the window's width. A height-only resize is
not one: Safari's toolbar resizes the height while scrolling. From the
first `orientationchange` or width-changing `resize` until 600 ms after the
last one, scroll events do not move `E` by the rule above. The bar keeps
the fraction of itself in view that it had when the turn began.

During the turn the bar is re-placed at the new view's top after each scroll
that lands: iOS's own scroll at the resize, the keeper's put-back
(`scrollerSwap.ts`), and the keeper's second put-back once the Continue row
lays out its columns. The keeper reads the bar's line from the bar's script,
not from the bar's box, which can be stale mid-turn. At the turn's end,
`E` is set from the held fraction against the new `y`.

The owner's log shows the order on the phone. Over the eight turns logged
with the bar at rest in the page, iOS's scroll came 53 to 126 ms after
`orientationchange`, the keeper's put-back within 1 ms of it, and the bar
is re-placed after both. The remaining "very slight pop" most likely falls
in that time between `orientationchange` and iOS's scroll, while the bar
is still placed against the old layout. The build makes one attempt to
close that window: it re-places the bar on the turn's first `resize` as
well. The owner judges the result on the phone at PR time.

During the 600 ms hold the bar is re-placed after each scroll rather than
carried by the track, so a reader who scrolls within 600 ms of a turn sees
it lag for that moment. Holding a turn's state is worth that. This is my
call (builder check).

### The keeper

`scrollerSwap.ts` takes the bar's bottom edge as the top of the reader's
view (around line 135). It now asks the bar's script for the line: the
bar's bottom in the viewport, `H` times the fraction in view, and 0 when
the bar is out of view.

### Anchors under the bar

`admin.css` and `settings.css` give jump targets
`scroll-margin-top: calc(var(--sticky-top) + …)`. They keep that margin. A
jump down hides the bar and leaves a gap of `H` above the target, which is
harmless. A jump up shows the bar, and the margin keeps the target clear of
it. This is my call.

## From the prototype to the PR

The prototype's code is the starting point, not the shipped code.

- **Keep:** the drawer's width rule; the A foot rule; the track, the `E`
  rule, the forced-shown cases, the rest switch, the turn hold, and the
  keeper reading the line from the script.
- **Drop:** `web/public/proto*` and the `index.html` hook; the B and C feet,
  including ThemeControl's second copy and AccountMenu's panel entries; the
  `layer`, `fixed`, `static` and Snap bars; `scrollerSwap.ts`'s
  `Math.max(0, …)` for a static bar; every `/proto-log` and
  `/proto-insets` fetch.
- **Name it for what it is:** the prototype's `protoHideBar.ts` becomes a
  module named for the bar (the builder picks the name), with the band rule
  as a pure function the tests can call.

## Constraints

- The server's content security policy runs no inline script and no inline
  style (`script-src 'self'`, `style-src 'self'`).
- The phone layout's condition comes from `phoneLayout.ts`. The CSS writes
  `(width < 900px) or (height < 500px)` letter for letter (O305's rule),
  and the foot's rule is `(height < 500px)`.
- Code comments say why, never history (project `CLAUDE.md`).
- Built CSS can differ from dev (O218), so the browser tests run against
  what Vite builds, as the existing browser project does.

## Testing

Browser tests (Chromium, the existing browser project) use real wheel input
(`web/tests/support/mouse.ts`), and each is watched failing against the
defect it names before it is trusted.

- **The band rule,** as a pure function: shown, partial, hidden; the clamp
  at 0 and at `maxScroll`; `E` never under `H`.
- **Track:** from pinned mid-page, down 20 gives a bar top of −20, down 100
  more gives −`H`, up 20 gives −(`H` − 20), up 100 gives 0 and pinned. At
  scroll 0 the bar is at 0. A scroll past either end changes nothing. A
  `requestAnimationFrame` sampler during a wheel scroll finds the bar's top
  never more than 1 px from `E − H − y`, clamped to `[−H, 0]`.
- **Forced in view:** opening the drawer mid-page with the bar out of view
  brings it fully in. Tab into the bar does too.
- **At rest:** after the scroll ends the bar is `relative`, with a rect
  within 0.5 px of the pinned one. The next upward wheel keeps its top at 0
  in every sampled frame. The next downward wheel carries it out with the
  content.
- **A turn,** emulated as a 440×956 ↔ 956×440 resize: the fraction in view
  is the same before and after, at rest and while tracking, and the keeper's
  put-back lands the reader's place as it does on main.
- **Focus and navigation:** after the drawer closes by a tap on a library,
  a wheel down carries the bar out of view (focus on ☰ does not hold it).
  Escape closes the drawer and leaves keyboard focus on ☰, and that holds
  the bar in view. A navigation mid-page, Back to a kept place among them,
  shows the bar fully.
- **Rest after the cases that are not a scroll:** after the drawer closes,
  after a navigation and after a turn, the bar in view is `relative`. After
  a sideways wheel on the Continue row it is `relative` again within
  150 ms.
- **The drawer:** at 956×440 the drawer is 14rem wide (headless Chromium
  has no inset). The foot's computed `position` is `static` below 500 px
  tall and `sticky` above. With eight libraries the navigation shows more
  of the list than today's foot allows, and More's panel opens whole with
  the drawer scrolled to its end.
- **The prototype is gone:** no `proto` file in `web/public`, and no
  `/proto-` request anywhere in `web/src`.

**On the phone, at PR time** (the owner, about 5 minutes): Track's feel,
upright and sideways; a turn mid-page at rest with the bar in view; a turn
with the bar out of view; More opened at the end of the drawer held
sideways.

## Docs

- `docs/status.md`: close O307, O309 and O337, each by the five-part edit
  (memory: open-items ritual). O337's row records what the owner judged on
  the phone at PR time, slight pop included.
- The style-pass spec's phone bar (§7.1, line 494, "pinned with
  `position: sticky`") gets a line saying this PR replaced it.

## Left to the builder

- Module, element and class names, and where in `Shell.tsx` the track goes.
- Whether the foot's media rule sits inside the phone layout's block or
  beside it.
- How the keeper and the bar's script share the line (an import, or a
  small module both read).
- How to emulate a turn's iOS scroll in a test. Task 13 of the resize
  polish imitated it (`web/tests/scrollKeeper.browser.test.tsx`), and that
  is the model.

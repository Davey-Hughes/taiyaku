# O140 — the chrome's build cost — design

**Status: settled in brainstorming on 2026-09-20. Not yet built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

O140 was opened on the style pass's ④ branch, from the user's own report that
the reader's menu was slow to open on their iPhone. The row offered two
candidate fixes — keep the chrome mounted once opened and hide it, or window
the rail — and recorded that both change what a test can assert about a hidden
bar. The user chose, on 2026-09-20, to profile first rather than build either.

This settles what the profile found, which of the candidates survived it, and
what the PR does.

## 0. How every figure below was measured

The app is `main` at `86b3533` served by vite (`:5199`) against
`taiyaku serve --db target/style-pass/lib/library.db` (`:18140`), the scratch
copy of the NAS library. Driven in Chromium through the Chrome DevTools MCP
server at a `390x844x1` emulated viewport with **4× CPU throttling** — the
same conditions ④'s device pass recorded in `notes.md`.

The volume is `akame-ga-kill----v01--2015--cbz`, **237 pages**, `rtl`. The
short volume used for the tile-count comparison is
`jujutsu-kaisen---v31--2026--cbz`, **20 pages**.

**Four things this did not measure, each of which could move a figure:**

- **`devicePixelRatio` is 1, not 3.** The phone is a 3× screen. Thumbnails are
  a fixed 160 px on the long edge regardless (O134), so the request count does
  not change, but paint cost would.
- **No touch emulation**, for the reason ④ recorded for declining `hasTouch`.
- **Thumbnails are warm** for every timing run below. The cold case is §5.
- **One machine, one run of each.** Medians of 5 or 10 with the spread given;
  no repetition across sessions.

The baseline reproduced ④'s recorded **element counts** exactly, which is the
reason to trust the rest: hidden, the reader is **30 elements and 1 image**;
shown, **774 elements, 238 images, 246 buttons** over **237 tiles**.

## 1. What the profile found

**The cost is React building 774 elements.** Not the images, not the fade, not
the re-centring.

| Re-show mechanism, 237 tiles, warm | median of 5 | spread |
| --- | --- | --- |
| Today — unmount, then remount | **274 ms** | 221–277 |
| `display: none` → shown | **15 ms** | 12–15 |
| `visibility: hidden` → shown | **4 ms** | 3–5 |
| `opacity: 0` → shown | **0 ms** | 0–1 |

And the cost scales with the tile count rather than being fixed: the 20-page
volume — 20 tiles, 114 elements — rebuilds its whole chrome in **38 ms**
(33–40), against 274 ms for 237 tiles.

### 1.1 Two things in the record that the profile contradicts

**`loading="lazy"` does defer thumbnail requests here.**
`docs/status.md` records, under "What M5b did not reach", that the tiles'
`loading="lazy"` "does not defer thumbnail requests at all in the environment
tested, so it bounds nothing about the size of a cold-strip burst". Measured
on this volume at 390×844: **101 of 237** thumbnails were ever requested.
Tiles from index 101 on stayed **4 px wide** — border only, no image — and
issued no request until scrolled to. The unmount's protection is real, but the
burst it protects against is 101 requests, not 237, and windowing's network
win is 101→~20 rather than 237→~20.

This does not reopen the earlier finding so much as bound it: that measurement
may have been taken on a shorter rail or a different viewport, where every
tile fell inside the lazy-loading margin. **Neither figure should be cited
again without saying which rail length and viewport it was taken at.**

**The DevTools "forced reflow" insight is a red herring.** The trace
attributes **274 ms of forced reflow** to `recenter` at `Filmstrip.tsx:135` —
the `scrollIntoView` the rail re-runs on every thumbnail's `load` in the
capture phase. Replacing `Element.prototype.scrollIntoView` with a no-op,
changing nothing else, prevented **103 calls** and moved the open cost from
**225 ms to 225 ms** (long tasks 361 ms → 351 ms). That layout happens
regardless; the insight names the function that triggers it, not one that
adds cost. **Optimising `recenter` would have bought nothing**, and the
insight alone would have sent someone to do it.

## 2. The decision, and what it beat

The user first chose mount-once-and-hide on the strength of §1's table. That
choice was then reopened by one more measurement, and the reopening is the
substance of this design:

| Page turn, 237 tiles | median of 10 | turns that moved |
| --- | --- | --- |
| chrome hidden — today's `return null` | **37 ms** | 9/10 |
| chrome shown | 178 ms | 8/10 |
| **mounted, `display: none`** | **165 ms** | 8/10 |

`display: none` saves 13 ms of a 141 ms penalty, because the cost is React
reconciling 774 elements and `display: none` does not stop a re-render. So
**mount-once alone is a 4.4× regression on the single most common interaction
in the app**, paid by every reader after their first `Tab` — against exactly
the page-turn latency O107, O110 and O111 spent three PRs buying back.

Two consequences:

1. Mount-once needs a **second** mechanism — the chrome must stop re-rendering
   while hidden, not merely stop being painted.
2. Windowing gained an argument nobody had made for it: it is the only
   candidate that touches the **178 ms page turn while the chrome is open**,
   which is a live cost on `main` that no row records. A reader who keeps the
   strip up pays it on every turn.

**The user chose both**, on 2026-09-20, with the regression and the second
mechanism stated.

| | first open | later opens | turn, closed | turn, open | cold requests |
| --- | --- | --- | --- | --- | --- |
| Today | 274 ms | 274 ms | 37 ms | 178 ms | 101 |
| Mount once + `display:none` alone | 274 ms | 15 ms | **165 ms** | 178 ms | 101 |
| Mount once + freeze | 274 ms | ~15 ms | 37 ms | 178 ms | 101 |
| Window the rail | 38 ms | 38 ms | 37 ms | ~50 ms | ~20 |
| **Both — what this builds** | 38 ms | ~15 ms | 37 ms | ~50 ms | ~20 |

**The `~50 ms` is a guess and says so.** It is extrapolated from the 20-tile
volume, where only **1 of 8** turns actually moved the page before the volume
ran out — so the scaling direction is measured and the figure is not. It is
one of the numbers §6 re-measures.

## 3. Windowing the rail

`Filmstrip.tsx`, and one new pure function in `tiles.ts`.

**A tile's width becomes derivable without its image.** Today a tile has no
width of its own: the thumbnail is `height: 100%; width: auto`, so the tile is
as wide as the image turns out to be, and a not-yet-loaded tile lays out at
4 px. That is what makes the rail impossible to window — nothing can say where
tile 150 starts without loading tiles 0 through 149.

```ts
/** How many nominal page-widths one tile occupies. */
export function tileUnits(group: Group): 1 | 2
```

Two units for a joined spread (`spread: true` — one image, twice as wide) or a
`two`-mode pair (two images side by side); one otherwise. It reads the same
`Group` that `tilePages` reads, so the two cannot disagree about what a tile
holds. It lives in `tiles.ts` beside `tilePages` for the reason that file's own
doc gives — a pure function sharing a file with a component defeats Fast
Refresh, and `biome.jsonc` enables `style/useComponentExportOnlyModules` by
name.

**The tile box becomes explicit**: `width: calc(var(--strip-unit) * units)`,
with the thumbnail `object-fit: contain` inside it. `--strip-unit` is **new**
and is derived from the existing `--strip-tile` in `styles/reader.css`, which
is a *height* — 160 px from 900 px up, 96 px on a phone — multiplied by the
2:3 nominal aspect the measured tile already sits at. At 390×844 that is the
54 px of image inside a 58 px button this profile measured.

**The one visible consequence, stated rather than discovered later**: a page
whose aspect is not the nominal letterboxes inside its tile instead of
widening it. Most pages will not move. This was chosen over measuring each
tile with a `ResizeObserver` and caching the results — that keeps the rail
pixel-identical, but the spacer widths shift as estimates are replaced by
measurements, which makes the rail jump under a reader while it fills. A
regular strip was judged the better failure.

**The window itself.** `Filmstrip` renders tiles `[lo, hi)` and two spacer
divs whose widths are the summed units of the tiles before `lo` and after
`hi`, each expressed as a `calc()` over `--strip-unit` so no tile is ever
measured.

**What `scrollWidth` does, stated precisely**, because a looser claim was
in an earlier draft of this section and was wrong: giving tiles an explicit
width *does* move `scrollWidth` off today's 8011 px, because today the rail
is as wide as its images happen to draw. Windowing itself then changes
nothing further. So the invariant to hold — and the one §7 tests — is
**windowed and unwindowed agree with each other**, not that either agrees
with 8011. `scrollIntoView` lands correctly because every tile, rendered or
spaced over, occupies exactly its nominal width.

The window is centred on the tile the existing effect already picks —
`currentRef`, else `pairedRef`, else `anchorRef` — plus a margin either side,
and an rAF-throttled `scroll` handler extends it to what the reader scrolls
to. Centring on that same tile is what makes the window's contract identical
to the one `Filmstrip`'s doc already states: whichever tile a reader needs on
screen for the state they are in is the tile the window is built around.

**What does not change**: `flexDirection: row-reverse` derived from
`stepFor(1, reading)`, the three refs and `isAnchor`'s re-test of the caller's
contract, `aria-current`, `onNavigate`/`onPickCounterpart`, the half badge,
and the hover preview.

## 4. Mounting once, and freezing

`Chrome.tsx`.

**Mount once.** `Chrome` keeps a `hasShown` flag and returns `null` exactly as
today until the first time `visible` is true. A reader who never presses `Tab`
therefore still fetches **zero** thumbnails. That is the whole reason the
unmount exists — `Chrome.tsx`'s own doc argues it, and §1.1 above found
`loading="lazy"` a weaker backstop than the doc's counter-argument assumed,
not a stronger one. Preserving it is not optional.

**Hide in place.** Once shown, both bars live inside one container whose
`display` is `none` while hidden. `display: none` rather than
`visibility: hidden`, although the latter re-shows in 4 ms against 15: a
`visibility: hidden` rail is still laid out, so its 8011 px of tiles reflow on
every page turn, which is the cost this design exists to remove. 11 ms on a
re-show is worth paying to keep the hidden chrome out of layout entirely.

**Freeze.** While hidden, `Chrome` renders the element tree it last built
rather than building a new one, so React skips reconciling the subtree. §2's
table is why: without it, the mounted chrome costs 165 ms on every page turn.

**The fade.** `scheduleFade` already schedules a frame so the first paint
lands at `opacity: 0` and the declared transition has something to animate
from — which is exactly what a `display` change also needs. The expectation is
that it keeps working unchanged. **That expectation is not a verification**:
jsdom cannot see a transition, so it has to be driven in a real browser before
the PR claims the fade survives.

Hiding still does not fade, and this design does not change that. Mounting the
chrome makes a fade-out *possible* for the first time — an element that has
unmounted has nothing left to animate, and one hidden in place does. Whether
the bars should fade out is a **design question for the style pass**, not a
consequence of this change, and it is deliberately not answered here.

## 5. What this does not fix

**The cold strip.** On a work whose thumbnails the server has not yet
generated, the first open fired 101 thumbnail generations and the strip was
**still filling after more than two minutes** — 101 of 237 tiles loaded, the
rest never requested. Windowing cuts the first burst to roughly 20, which is
the real improvement, but the per-tile generation cost is O11's and O9's
subject and O134 is about to multiply it by sending a larger tile to a dense
screen. Nothing here changes `cover.rs`.

**The 178 ms page turn with the chrome open** is expected to fall to roughly
50 ms and is not expected to reach the 37 ms of a closed chrome. If the
re-measurement in §6 shows it has not moved as far as §2's table guesses, that
is a new row, not a reason to hold this PR.

## 6. What gets re-measured before it merges

The same five numbers, under §0's conditions, on the same two volumes, with
the before-figures above as the comparison:

1. First open, 237 tiles — 274 ms today.
2. A later open, 237 tiles — 274 ms today.
3. A page turn with the chrome closed — 37 ms today.
4. A page turn with the chrome open — 178 ms today.
5. Thumbnail requests on a first open — 101 today.

And two that are not numbers:

6. **The fade still plays on showing**, driven in a real browser.
7. **A reader who never presses `Tab` still requests zero thumbnails** — the
   protection §4 keeps, checked rather than assumed.

## 7. What the tests have to say instead

The candidates were recorded in O140's row as "both changing what a test can
assert about a hidden bar". They do, and this is the list.

- **`Chrome.test.tsx:71` inverts.** "renders nothing while hidden — an unmount,
  not merely invisible chrome" becomes "renders nothing **before the first
  show**", joined by a new case asserting the DOM survives a hide.
- **A freeze test that fails when the freeze is removed.** Count `Filmstrip`
  renders across a page turn while the chrome is hidden; assert zero. A freeze
  that silently thaws looks identical on screen and costs 165 ms a turn, so a
  test that only reads the DOM would pass against the defect. Per the project's
  rule, the defect gets hand-applied and the test watched to fail.
- **Windowing**: only a window of tiles is rendered; the current, paired or
  anchor tile is always inside it; **the windowed rail's `scrollWidth` equals
  the same rail rendered unwindowed** — not that it equals today's 8011 px,
  which the explicit tile width moves; the `rtl` case, which is where a
  fixture can hide a docking defect (O65); scrolling extends the window.
- **`tileUnits`** unit-tested directly in `tiles.test.ts`, beside `tilePages`,
  including the joined-spread and `two`-mode-pair cases that give 2.
- **`App.test.tsx:930`** — `expect(button(host, 'two')).toBeUndefined()` before
  `Tab` — still holds unchanged, because nothing is mounted before the first
  show. Any test that toggles the chrome off and then asserts absence becomes
  an assertion about hiddenness; the sweep for those is part of the work, not
  a claim that §7 has already found them all.

## 8. Build order

Two commits, one PR:

1. **Window the rail.** Independently valuable, and it does not touch the
   chrome's mount/unmount, so §2's regression cannot appear in this commit.
2. **Mount once, and freeze.** Built on a rail that is already cheap, so the
   freeze is protecting a ~114-element subtree rather than a 774-element one.

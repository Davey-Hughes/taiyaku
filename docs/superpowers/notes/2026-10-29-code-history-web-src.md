# Code history moved out of comments

Text moved verbatim out of code comments when they were trimmed to their why. Each entry names the file and the line of code the comment sat above.

## `web/src/App.tsx`, above `const gate = useGate()`

What stood here instead was `forgetTheReader`: a `useCallback` resetting
thirteen pieces of state and cancelling two debounced writers, registered
as `api.ts`'s `whenSignedOut` handler and called by the account panel's
Sign out.

## `web/src/Borrowed.tsx`, above `<div`

A bar pinned across the foot of the page area was tried first and
measured against real art (`gotoubun-v01`, `en-mono`, hub slot 6, at
1400×620): at that height it covered roughly the bottom 13% of a
colour spread half, sitting over the largest text on the page, with
its white text competing against whatever artwork was behind it.

## `web/src/Filmstrip.tsx`, above `const guard = Math.max(rb.width / 4, 200)`

measured at 390×844
under 4× CPU throttle a 97 px guard (390/4) leaves a spacer briefly
visible for 3 frames, up to 18 px, while 1440×900 sees none with its
360 px proportional guard.

## `web/src/Filmstrip.tsx`, above `useEffect(() => {`

The rAF throttle above cuts a scroll's *event* rate, but a 1.2 s flick
still produced 65 window rebuilds across 181 throttled frames measured
this way (O140), because the focus itself moves by roughly a tile on
nearly every frame of a fast flick — a rebuild for every tile of drift
is expensive on its own. A first fix held the anchor still until the
focus had drifted a fixed tile count from it, sized so that count plus
the viewport's own reach stayed inside `windowMargin`. That reasoning
was wrong: it assumed the viewport straddles the focus tile roughly
evenly, when `focusAtFraction` actually returns the tile at the rail's
*leading* edge for the scroll fraction, and the viewport extends forward
from there — about a dozen tiles at 1440 px, not ~7 either side. Measured
at 1440×900 during a fast scroll, that mistake showed up as a spacer
inside the visible strip on 7 frames, the worst one a 321 px gap in a
1440 px rail.

Measured driving a
1.2 s flick at 390×844 under 4× CPU throttle: 71 re-anchors across 182
frames, worse than the 65 the drift-based rule above produced and worse
than the 24 the spacer rule below achieves.

## `web/src/Filmstrip.tsx`, above `const windowMargin = 16`

A 237-tile rail with `windowMargin` at 12 built about 25 tiles per change
(O140: 237 tiles cost 274 ms, 20 tiles 38 ms); at 16 a change builds about
33

## `web/src/GroupView.tsx`, above `<div`

**A reader noticed, and the measurement
agreed with them:** a median 89.7 ms between the two cells at a slow
reading pace, 158.4 ms at worst, every one of eight turns tearing by
more than a frame, and 213.8 ms median under a flick.

## `web/src/cohort.ts`, above `export interface CohortMember {`

Measured in the browser against `gotoubun-v01` with a warm cache: a median of 89.7 ms between the two cells' `src` changes at a slow reading pace, 158.4 ms at worst, and **every one of eight turns tore by more than a frame**. Flicking at 120 ms between turns took the median to 213.8 ms and the worst case to 1.28 s.

## `web/src/modules/prefetch.ts`, above `defaults: { enabled: true, ahead: 1, behind: 1, maxConcurrent: 3 },`

Measured in the browser against
`gotoubun-v01` `en-mono`, keypress to both halves on screen, 20 turns per
setting:

| window | median | p90 | worst | turns over 100 ms |
| ------ | -----: | --: | ----: | ----------------: |
| `ahead: 2, behind: 1` — 6 slots | 14 ms | 232 ms | 335 ms | 29% |
| `ahead: 1, behind: 0` — 2 slots |  4 ms | 125 ms | 182 ms | 10% |
| `ahead: 1, behind: 1` — 4 slots |  4 ms |  70 ms | 216 ms |  5–20% |

The same effect from the other side: raising `maxConcurrent` from 3 to 6,
so more of that window decodes at once, made it *worse* (5 slow turns in 16
against 3 in 14) rather than better.

**All of those runs warmed pages at full size.** Prefetch built its URLs
with no long edge until O110 closed on 2026-09-14, so each `en-mono` page
it warmed decoded to about 26 MB, where one fetched at 1024 decodes to
about 6 MB (`pageSize.ts`'s table). Neither depth nor `maxConcurrent` has
been measured against the smaller decode.

though the two measure alike forward

the run above that reversed every fourth turn is the one `behind` exists
for.

## `web/src/pageSize.ts`, above `export const SIZE_LADDER: readonly number[] = [512, 768, 1024, 1536, 2048, 3072]`

**The measurement that put this here.** `gotoubun-v01`'s `en-mono` pages are
2147×3056 — 6.56 megapixels — and in a 1385×512 window the reader draws them
into a box of 693×475. Twenty times the pixels that reach the screen. What
that costs, measured in the browser with the bytes already cached so the
timing is decode alone:

| fetched at | pixels | decode | decoded bitmap |
| ---------- | -----: | -----: | -------------: |
| 2147 (as released) | 6.56 MP | 74.5 ms | 26.2 MB |
| 1600 | 3.64 MP | 14.9 ms | 14.6 MB |
| 1024 | 1.49 MP |  7.3 ms |  6.0 MB |
| 700  | 0.70 MP |  4.3 ms |  2.8 MB |

## `web/src/modules/fit.ts`, above `export function pageStyle(`

Measured on `gotoubun-v01` in two-page
mode with the defaults, in a browser on 2026-08-08: the two `<img>`
*elements* already touched — 0px between them — while their artwork sat
**359px apart**.

`100% 50%` on the left
cell and `0% 50%` on the right took that 359px to **0px**, measured the same
way.

## `web/src/rules.ts`, above `export const PREVIEW_EXPENSIVE_ABOVE = 40`

measured in a debug build on a one-work fixture: 10
extra rules 47 ms, 40 → 401 ms, 80 → 1.34 s, 160 → 5.01 s.

## `web/src/styles/admin.css`, above `.proposal-states .state {`

Measured in Chromium at 390x844 with `hasTouch`, against the built
stylesheet, cap-top offset from the pill's optical centre:

  padding-block      Open    Accepted  Dismissed
  4px 4px (base)     +1.5    +1.5      +1.5      all low
  0 6px              -1.5    -1.5      -1.5      all high
  2.5px 5.5px         0.0     0.0       0.0      centred

## `web/src/styles/admin.css`, above `min-width: 10rem;`

10rem is where that stops improving, measured at 390 across 8/10/12/14:
the median row falls 933 -> 93 -> **72px** and then stays at 72, while
the box's own scroll keeps growing (239 -> 271 -> 303 -> 335). So 10rem
buys the whole of the improvement and the least of the cost. The table
comes to 15,630px, a tenth of what it was.

## `web/src/styles/library.css`, above `.alignment-panel .paired {`

measured (Chromium system colours, `CanvasText` at
this opacity over `ButtonFace`) at the previous `0.75`, **3.81:1 dark /
9.63:1 light** — dark fails.

## `web/src/styles/admin.css`, above `.preview .table-scroll {`

Measured before this existed, at a 390px viewport where driven Chromium
leaves 375px of client width: `.provenance` rendered 524px and `.matched`
393px, for 165px and 34px of document overflow. Only one is ever in the
DOM, so each was found on its own tab.

## `web/src/styles/admin.css`, above `.preview .table-scroll th:first-child,`

(**36 characters is where the 762 was counted**, and it is a sharp edge:
757 at 35, 824 at 37. It stands for the 16rem this cell used to be capped
at — 256px over roughly 7.1px a glyph at `--text-ui`. Earlier commits
wrote the figure as "cut to 16rem", which is the same measurement told in
the units the cap was written in rather than the ones it was counted in.)

## `web/src/styles/admin.css`, above `min-width: 10rem;`

390 that was **38px — about three characters**. A 67-character path
wrapped into that ribbon is 44 lines: measured, a Provenance row stood
**933px tall**, taller than the 844px viewport, so a single row did not
fit on screen, and the table came to **161,451px** over 224 rows.

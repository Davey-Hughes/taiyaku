# O134 — a sharper strip tile — design

**Status: settled in brainstorming on 2026-09-21. Not yet built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here says where it came
from, and a guess says it is one.

O134 was opened on the style pass's ③ design, 2026-09-19: the reader's page
strip is soft on a phone. ④'s device pass — the first to put the strip on a
phone — asked the user which of three answers they wanted, and on 2026-09-20
they chose **the server sending a sharper tile, as O133 does for covers**
(`docs/notes.md`, "What the user answered"). The row stayed open as its own PR
because the change is `cover.rs`'s and it multiplies the per-edition tile cache
O11 records.

This settles what "as O133 does" means here, since a strip tile and a library
cover differ in the one way that matters: a cover is page 0 and a strip has one
tile per page.

## 0. Where the numbers below come from

**Arithmetic on CSS constants and `devicePixelRatio`, not rendered
measurements.** `--strip-tile` is `160px`, and `96px` under the 900 px
breakpoint (`styles/reader.css:52,130`); the proposals inbox's tile is `4.5rem`
wide at `aspect-ratio: 2 / 3` (`styles/admin.css:298`), so 72×108 CSS px; the
credits panel's is `height: 56` (`CreditsPanel.tsx:30`). A tile's image is
`THUMB_LONG_EDGE`, 160 px, whatever it is drawn at.

| drawn at | wanted at 3× | served | upscale |
| --- | ---: | ---: | ---: |
| strip, phone (96 px tall) | 288 | 160 | **1.80×** |
| strip, desktop (160 px tall) | 480 | 160 | 3.00× |
| proposals inbox (108 px tall) | 324 | 160 | **2.03×** |
| credits panel (56 px tall) | 168 | 160 | 1.05× |

The user saw the first row on their own iPhone. The others are this table's
arithmetic and nobody has looked at them on a 3× screen; the credits panel's
1.05× is the reason it is left alone below.

**What is not measured yet, and gates the row closing**: what a tile costs in
bytes at each size, which is the figure O11 carries. §8.

## 1. What O133 already built, and what it deliberately did not

`cover.rs` caches a tile at `{sha256}-p{offset}-{edge}v{version}.jpg`
(`thumb_keyed_path`) — **the long edge is already in the path**. A second size
is therefore a new file and never a stale one: every tile cached today stays
valid and is still served, and no cache migration exists to get wrong. This is
the whole reason the change is small.

O133 made `?w=` work on the tile route for **page 0 only**, at exactly
`COVER_LONG_EDGES` (`routes.rs::cover_edge`), and its doc comment says why: the
tile route skips `refuse_if_cannot_stream`, so `?w=` on any page at any size
would hand an account that may browse and download but not read pages "every
page at a size worth reading". O134 needs exactly the thing that comment
forbids, so the first decision is where the line goes instead.

## 2. Decision 1 — sized tiles require `can_stream`; the fallback is the 160 px tile

**Chosen.** Page 0 at a cover size stays exactly as O133 left it, open to every
account. Any other page carrying a tile `w` is sized **only if the principal may
stream**; otherwise the request is served as today's 160 px tile.

- It costs such an account nothing real. `refuse_if_cannot_stream` guards
  `get_page` (`routes.rs:852`) and one OPDS route, and nothing else — so a
  download-only account is answered `403` for every page and its reader has
  nothing in it. A sharper strip over a blank reader is not a loss.
- The fallback, rather than a `403`, reuses `cover_edge`'s existing rule that
  anything not a cover is the tile. No new status code, no new failure mode, no
  broken image: an unknown `w`, a non-numeric `w` and an ineligible account all
  land on the same 160 px answer.

**Beaten: no check at all, capping the size instead** — any page at ≤480 px for
anyone, on the argument that a 480 px manga page has unreadable text. It is one
code path and no new branch, and it replaces a rule ("may not read pages here")
with a judgement about legibility that nothing in the code can check.

**Beaten: `403` rather than the fallback.** Louder and easier to assert, at the
cost of a broken-image strip where a blurry one would do.

## 3. Decision 2 — a ladder chosen at runtime, like `pageSize.ts`

**Chosen.** `TILE_LADDER = [160, 320, 480]`, and the client rounds
`drawn height × devicePixelRatio` up to a rung, exactly as
`pageSize.ts::requestedLongEdge` does for a full page.

```
phone   96 × 1 =  96 → 160      desktop  160 × 1 = 160 → 160
phone   96 × 2 = 192 → 320      desktop  160 × 2 = 320 → 320
phone   96 × 3 = 288 → 320      desktop  160 × 3 = 480 → 480
```

Three rungs cover every case in §0's table, and the strip's two heights are why
the pick has to happen at runtime rather than in markup.

**Beaten: `srcSet` with `1x/2x/3x` descriptors, as `coverSources` does.** It is
the same idiom as O133 and needs no JavaScript — but an `x` descriptor cannot
see the 900 px breakpoint, so a 3× phone would fetch 480 where 320 is enough:
2.25× the bytes and 2.25× the cache growth on the exact device that opened this
row. A `<picture>` with a media query fixes that and adds two nodes to each of
the ~33 tiles O140 windows.

**Beaten: raising `THUMB_LONG_EDGE` to 320 for everyone.** About ten lines, no
route change, and decision 1 evaporates. It also makes every 1× desktop fetch
~4× the bytes for nothing and invalidates every cached tile in every library by
construction. Rejected as a worse trade, not as wrong.

## 4. The server

```rust
// cover.rs
/// The long edges a filmstrip tile is offered at, one per screen density.
pub const TILE_LONG_EDGES: [u32; 3] = [THUMB_LONG_EDGE, 320, 480];

pub fn cached_or_generated_thumb(dir, sha256, offset, edge: u32, page) -> …
```

`cached_or_generated_thumb` gains the edge and passes it to both
`thumb_cache_path` and `thumbnail_at`, which is the pairing the cache's
correctness rests on — the file it writes and the size it generates must come
from one argument, never two.

`routes.rs::get_thumb`, after the existing cover branch:

```rust
let edge = tile_edge(first_query_value(&query, "w"))
    .filter(|_| can_stream(&request_extensions))
    .unwrap_or(cover::THUMB_LONG_EDGE);
```

`tile_edge` parses and keeps only a member of `TILE_LONG_EDGES` — the same
shape as `cover_edge` without the page-0 test.

**`can_stream` is new and `refuse_if_cannot_stream` is rewritten to call it**,
so the rule has one statement rather than two that can drift. It must keep that
function's existing behaviour when there is **no `Principal` in the
extensions** — which is "allowed" — or every request in a configuration without
auth silently drops to 160 px and no test that logs in would see it.

The tile keeps sending no `ETag`; the reasoning in `get_thumb`'s doc comment is
unchanged by any of this.

## 5. The client

**A new `tileSize.ts`**, mirroring `pageSize.ts` — its own module rather than a
function in `tiles.ts`, because `ProposalRow.tsx` needs it too and the strip's
geometry is not an admin screen's dependency.

```ts
export const TILE_LADDER: readonly number[] = [160, 320, 480]
export function requestedTileEdge(tileHeightCss: number, dpr: number): number
```

Two rules deliberately differ from `requestedLongEdge`, and the module says so
in place:

- **Unusable input → 160**, not the original bytes. A page falls toward full
  resolution because soft is the failure mode there; a tile falls toward small
  because 237 of them at 480 px is the failure mode here.
- **Above the top rung → 480**, clamped, rather than `undefined`. A 4× display
  wants 640 and there is nothing better to offer it. `requestedLongEdge`
  answers `undefined` there because the original page exists; no original tile
  does.

It returns a number always, so `thumbUrl` always carries a `w`.

**The server holds the same three** (`cover.rs`'s `TILE_LONG_EDGES`), and the
module says so where `library.ts` says it of `COVER_LONG_EDGES`: a rung added
here alone is not a sharper tile, it is a `w` the route does not recognise,
answered with the 160 px tile — soft rather than broken, and so invisible
without a test that decodes what came back.

**The height comes from `matchMedia`, and the constants are restated in
TypeScript** — `STRIP_BREAKPOINT_PX` and `STRIP_TILE_PX` in `tileSize.ts`,
read during the component's first render through a lazy `useState`.

**Beaten: reading `--strip-tile` back out of the CSS** with
`getComputedStyle(rail).getPropertyValue('--strip-tile')`, the call
`Filmstrip.tsx`'s mount effect already makes for `--strip-tile-border`. That
needs no restatement at all, which is the better property — and it cannot be
done in time. The rail must be in the document before it can be measured, so
the earliest the answer exists is the mount effect, one commit after the first
render has already put a `src` on every tile. Every visible tile would fetch at
160 px and then again at its real rung: the cost this row exists to remove,
doubled. The restatement is the price of answering during the first render.

**What pays for the restatement** is `styles/reader.css.test.ts`, which parses
the stylesheet and pins `--strip-tile`'s two values *and* the breakpoint
against these constants, so the two files cannot drift apart silently. That
test is the load-bearing half of this decision rather than a courtesy.

*(This section originally specified the `getComputedStyle` reading as what
would ship, on the drift argument alone. The first-render timing was found
while writing the implementation plan, before any code; the plan carries the
shape that was built. Recorded here rather than quietly rewritten, since a
design document that silently matches the code teaches nothing about the
decision.)*

**This is a height, and O145 is about a width.** The 15 px O145 cannot explain
is subtracted from the rail's height and shows up in the drawn tile *width*;
rounding up to a rung absorbs a discrepancy of that size at every step of the
ladder, so O134 neither answers O145 nor depends on it.

**`thumbUrl` gains an optional fourth argument.** It cannot route through
`pageUrl`'s `?w=` — that would put the query before `/thumb` — so it appends its
own. Called with three arguments it is unchanged, which `api.test.ts:43`
already asserts.

## 6. The proposals inbox

`ProposalRow.tsx` passes `requestedTileEdge(108, devicePixelRatio)`: 108 is the
CSS box, a constant, so no measurement and no effect. This closes the clause
O133 left in its row — "the filmstrip, and the proposals inbox's page tiles,
keep the 160 px tile".

**The credits panel is left alone.** 56 px × 3 = 168 against 160 is 1.05×; a
rung there would fetch 320 to fix nothing anyone can see.

## 7. Tests

The one that matters most is the one O133 shipped without: **a download-only
account asking `?w=480` for page 5 gets the 160 px tile.**
`download/tests.rs::without_can_stream_the_pages_are_403_and_the_download_still_works`
asks only for the *unsized* tile (`/pages/0/thumb`, no `w`) — checked on
2026-09-21, and no other test pairs a sized request with such an account. That
gap is exactly how the first version of O133's cover path reached review
refusing every cover on such an account with the gate green.

Server:

- each rung comes back at that long edge, **decoded and measured**, not asserted
  from a header;
- a `w` that is not a rung, and a `w` that is not a number, are the 160 px tile;
- page 0 at a cover edge still goes through `serve_page` — unchanged, and the
  existing test proves it stayed so;
- `can_stream` answers `true` with no `Principal` present, pinned on its own,
  because that is the branch a logged-in test cannot reach.

Web:

- `requestedTileEdge` over §3's table, plus both fallback rules, in the `node`
  project;
- **the strip's `src` carries the rung, asserted in
  `Filmstrip.browser.test.tsx`** where the stylesheet is real. jsdom resolves no
  media query and no custom property cascade, so the same assertion there would
  pass against any value — the failure this project has hit repeatedly and
  records in `docs/status.md`.

Each of these is written to fail against the defect it names, and the defect is
hand-applied and watched to fail before the branch is called done.

## 8. What must be measured before the row closes

Into `docs/notes.md`, under "O134":

1. **A tile's bytes at 160, 320 and 480** for a real edition, and the per-edition
   total at each. O11 records 1.12 MiB per 195-page edition at 160. Extrapolating
   from O133's two measured points — a 160 px tile about 7 KB, a 768 px cover
   about 100 KB, so bytes go roughly as area to the 0.85 — puts 320 near
   **3.6 MiB** and 480 near **7.2 MiB** per 195-page edition. That is an
   extrapolation across one edition's artwork from two numbers, which is the
   reason to measure it rather than the answer.
2. **Cold generation time per rung** — one resize per (page, size) the first time
   it is asked for.
3. **The strip on the user's iPhone, before and after**, over the tailnet against
   a production build. It is the only evidence that the row's complaint is fixed;
   everything above is arithmetic.

## 9. The five-part edit, plus one

The ritual in `docs/status.md`: the O134 row's checkbox, its prose, the test
counts at both sites, the "N closed — leaving M" sentence, the "What remains"
line — then `grep` the bare `O134` across `docs/` and `web/src/`.

**And O11's annotation**, which reads "filmstrip tiles at 1.12 MiB per 195-page
edition" and stops being true here. O11's trigger is "the filmstrip's
thumbnail-per-page cache" growing; this multiplies it, so the row is re-read
rather than only re-numbered.

## 10. What this does not do

- **Nothing evicts the tile cache**, before or after. O133 left the same
  sentence for covers. This branch makes the number bigger and does not change
  who is responsible for it (O11).
- **O144's six mis-marked double-page scans** still draw at two units while
  `tileUnits` calls them one. A sharper tile does not move that.
- **O145** stays open, for §5's reason.
- **`--strip-tile` stays 160/96.** This is about the pixels in the image, not
  the size of the tile on screen.

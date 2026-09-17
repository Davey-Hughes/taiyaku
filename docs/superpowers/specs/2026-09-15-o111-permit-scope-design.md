# O111 — a permit that bounds decodes is held by requests that never decode: build design

**Status: settled in brainstorming on 2026-09-15.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O111**.

**Amended 2026-09-16, before any code, by the pre-flight check; this copy
governs.** Three things the brainstormed version did not see, each marked in
place below:

1. **A test pins the ordering this changes.**
   `cover/tests.rs::a_cold_reading_page_waits_for_a_generation_permit`, through
   its helper `fetched_while_every_permit_is_held`, asserts that a cold reading
   page does not *fetch* while every permit is held. After this change a page
   that needs a resize does fetch then, by design, so the test is rewritten to
   watch the resize finish instead. It must still go red when the permit around
   the resize is deleted, which is what O105's closed row pins ("the permit
   deleted at either site"). **This reverses no closed decision:**
   `cached_or_generated_page`'s own doc already said "Moving the fetch out of
   the permit is a separate change". `at_path`'s test stays as it is.
2. **Doc comments describe the old ordering**, and not only
   `cached_or_generated_page`'s "Only a cache hit skips the permit". `Semaphore`
   and `MAX_CONCURRENT_GENERATIONS` both define a generation as "fetching a page
   and decoding it", which stays true for covers and tiles and stops being true
   for reading pages.
3. **The O107 timing script was never kept.** §4's measurement is rebuilt from
   `docs/notes.md`'s description, and kept this time.

**And one found while building, 2026-09-16:** §2's "`fit_at` stays `pub`"
cannot hold. Once `cached_or_generated_page` calls the halves, `fit_at` has no
production caller left, and the workspace's `-D warnings` clippy gate refuses
it as dead code. It is removed. Its two halves are `fits_within` and
`resize_to_long_edge`, and its doc comment's arguments move onto them.

**And a second, 2026-09-17:** an `App.tsx` comment called a full-size page "a
full-resolution generation behind `cover.rs`'s two-permit `GENERATION_LIMIT`".
A full-size request has never taken a permit, and the comment is corrected.

## 0. The gap in one paragraph

`cover::cached_or_generated_page` serves a cache hit before it takes anything.
On a miss it acquires `GENERATION_LIMIT` — two permits, process-wide, shared
with every cover and filmstrip tile — and *then* fetches the page bytes and
*then* parses the JPEG header in `fit_at` to decide whether a resize is needed
at all. A page already at or below the requested long edge does no decoding
whatever, and still queues for a permit to find that out. Measured while
timing O107 on this workstation on 2026-09-13: with 187 `en-mono` tiles
requested cold over six connections, a fitting `raw-mono` page at `?w=1024`
answered in a median of 163.8–166.5 ms across three runs, against 2.9–3.1 ms
with no filmstrip; the full-size request beside it stayed at 1.6–1.8 ms and a
cache hit at 0.25–0.32 ms. Re-measured after O107's fix landed: 170.1 and
169.7 ms — that fix removed a read, not the queue.

## 1. Why this is a defect rather than the decision the row calls it

O111's row files this as *"a decision, not a defect"* and names two shapes.
That framing is narrowed here, and the narrowing is the reason the cheaper
shape can be taken without the measurement the row asks for.

**`GENERATION_LIMIT` is justified by decode memory and by nothing else.**
`cached_or_generated_page`'s own doc comment says so, and cites
`MAX_DECODE_ALLOC` as the quantity being bounded. A request that fetches bytes
and finds them already small enough allocates no decode buffer, so it is
holding a permit that exists to ration something it never asks for. **The
full-size path already agrees**: it fetches with no permit at all, and nothing
has ever proposed giving it one.

So moving the fetch and the header parse out of the permit is not a trade
between latency and memory. *(Overstated, found by the pre-merge claim check on
2026-09-17 and left as written: §2 below calls the queued fetched bytes "a real
regression in peak memory under load", so it is a trade whose memory side is
unmeasured.)* It is making one path consistent with the other,
and with the permit's own stated purpose. The number above says how much it is
worth; it is not what decides whether it is right.

**What the row's second shape would have cost, and why it loses.** Giving
reading pages a budget of their own targets the symptom directly and breaks
the property `cached_or_generated_page`'s doc comment argues for: one budget
over covers, tiles and pages is what bounds peak memory. Two budgets bound
two things and their sum bounds nothing, which is the whole reason the single
semaphore exists.

## 2. What changes

`cached_or_generated_page` becomes: cache hit, then fetch, then header parse,
then — only if a resize is actually needed — take the permit and decode.

```rust
let path = page_cache_path(dir, sha256, offset, long_edge);
if let Some(found) = cached(&path) {
    return Ok(Fitted::Resized(found));
}
let fetched = page()?;                           // no permit
if fits_within(fetched.as_ref(), long_edge)? {
    return Ok(Fitted::Original(fetched));        // no permit, no decode
}
let bytes = {
    let _permit = GENERATION_LIMIT.acquire();
    resize_to_long_edge(fetched.as_ref(), long_edge)?
};
```

This requires splitting `fit_at` into the part that reads the header and
answers *whether* a resize is needed, and the part that performs it. The
function already has that seam: `into_dimensions` parses the SOF marker and
`width.max(height) <= long_edge` returns `Ok(None)` at `cover.rs:183-185`,
with the decode, resize and encode below it. **The split is contained because
`fit_at` has one production caller** — `cached_or_generated_page` itself, at
`cover.rs:258` — and one test caller at `cover/tests.rs:482`, which is why
this is the whole of the code change rather than the start of it. ~~`fit_at`
stays `pub` and keeps its current behaviour for that test; the two halves are
what the permit moves between.~~ *Amended while building: with no production
caller left, `fit_at` is dead code under `-D warnings`, so it is removed. The
halves are `fits_within`, the header decision, and `resize_to_long_edge`, the
decode, and the test caller uses `resize_to_long_edge`.*

**The cost the row names, stated rather than hidden.** A page that *does* need
a resize now holds its fetched bytes in memory while it queues, where before
it queued first and fetched afterwards. That is a real regression in peak
memory under load, and it is bounded by the same two permits plus however many
requests are queued — which is why it is acceptable and not free. It is also
narrower than it sounds, for the reason the row gives: the full-size path
already holds fetched bytes with no permit, so this is not a new shape in this
file.

## 3. What this does not change

`cover::at_path` keeps taking the permit for every cover and filmstrip tile it
generates; those always decode. `MAX_CONCURRENT_GENERATIONS` stays at 2. No
budget is added, split or resized. The cache path, its contents and
`cached`'s whole-JPEG check are untouched.

## 4. Verification, which is the ritual and not an extra

O110 closed *"once a page turn had been timed before and after"*, and O65
*"once the reader had been driven before and after."* This follows both.

**The comparison is valid on this workstation**, which is worth one sentence
because the row says the deciding measurement belongs on the production host.
The 163.8–166.5 ms figure was itself *"this workstation, not the NAS"*, so a
before/after taken here is compared against the number it is trying to move,
under the conditions that produced it. A NAS figure would be a different and
additional fact; it is not required for this comparison to mean anything.

The measurement to repeat, from `docs/notes.md` under O107, "The permit wait"
(*amended: that run kept no script, so it is rebuilt from the description and
kept this time, and the notes cite it*):
187 `en-mono` tiles requested cold over six connections, a fitting `raw-mono`
page at `?w=1024` timed beside them, three runs, median reported. Expected
after: the fitting page's median falls toward the no-filmstrip figure of
2.9–3.1 ms, because it no longer queues. A page that needs a resize is timed
in the same run and is expected **not** to improve — if it does, the
measurement is measuring something other than what this changed.

## 5. Tests

- A request for a page already within `long_edge` completes while both permits
  are held by other work. `cover/tests.rs` already holds both permits
  deliberately (`(0..MAX_CONCURRENT_GENERATIONS).map(|_| GENERATION_LIMIT.acquire())`),
  so the harness for this exists; this is the same shape asserting the
  opposite outcome for the fitting case.
- A request that *does* need a resize still blocks while both permits are
  held — the guard that stops this change quietly removing the bound.
  *Amended: this is the existing `a_cold_reading_page_waits_for_a_generation_permit`,
  rewritten, not a new test. It watched the fetch, which now happens while
  the permits are held, so it watches the resize finish instead. It is renamed
  `a_cold_reading_page_that_needs_resizing_waits_for_a_generation_permit`, and
  its helper `fetched_while_every_permit_is_held` is now
  `flagged_while_every_permit_is_held`, since for a page the flag marks the
  finish rather than the fetch.*
- `Fitted::Original` is still returned for a page that needs no resize, and
  nothing is written to the cache for it.
- The split of `fit_at` preserves its answers: same bytes out for a page that
  needs resizing, same `None` for one that does not. *Amended: `fit_at` is
  gone, so this pins the boundary instead — a page equal to the request fits
  and one a pixel over is resized — through the halves and through
  `cached_or_generated_page`.*

## 6. Risks

- **The permit is process-wide and static, so tests that take it interact.**
  `cover/tests.rs` says so already and runs them accordingly under nextest.
  Any new test here inherits that and must not leak a permit on a failure
  path.
- **A test asserting "completes while permits are held" can pass against its
  own defect** if the permit was never contended in the first place. Both new
  tests must be run against a hand-applied mutation — put the permit back
  around the fetch and confirm the fitting-page test fails — before either is
  believed. `docs/mutants-baseline.md` records that the harness generates no
  mutant for some of these shapes, so the hand check is the real guard.
  *Amended after building: one hand mutation survives, and it is not new.*
  Binding the guard to `_` instead of `_permit` drops it the moment it is
  taken, so the resize waits for a free permit but does not hold one while
  it decodes. Every test waits on the acquire and none can see the release,
  so all pass. `at_path` has had the same exposure since the permit was
  added. Seeing it would take sampling the permit count during a decode,
  which is a timing test. *(Both overstated, found by the second pre-merge
  claim check on 2026-09-17 and left as written: `at_path` takes its permit
  the same way but was not mutated, and its fetch runs inside the permit, so a
  held fetch closure could show it without timing anything. Filed as O129.)*
- **Peak memory under load rises**, per §2. Nothing here measures it, and this
  is the guess this document is required to label: the expectation is that two
  permits' worth of queued fetched bytes is small beside `MAX_DECODE_ALLOC`,
  and it has not been measured. *(The wrong quantity, found by the pre-merge
  claim check on 2026-09-17: the permits bound the decodes, not the queue, so
  what is held is one fetched page per queued request, as §2 says.)*

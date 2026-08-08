# L1 geometry — design

**Status: settled in brainstorming on 2026-08-07. Nothing built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here is a measurement
already recorded in `notes.md`, and a guess says it is one.

## What L1 turned out to be, and where the existing note is wrong

`docs/status.md` carries L1 in its **Later** list as:

> **L1 geometry** — affine transform per pair, so a swap preserves zoom and pan.

Two things in that line do not survive contact with the code.

**There is no zoom and no pan.** `PageView` draws one page fit-whole —
`max-width`/`max-height` at 100%, `width`/`height: auto`, so box size and
content size coincide and there is nothing for `object-fit` to reconcile.
Nothing in `prefs.ts` or `settingsPanel.ts` mentions zoom. The clause "so a
swap preserves zoom and pan" describes a payoff for a feature that was never
built, which means **L1 is mostly a reader milestone with a geometry problem
inside it**, not a geometry milestone with a reader consequence. That
inversion is the single largest thing this document settles.

**"Per pair" prejudges the storage question, and the answer is provisionally
no.** `sidecar.md`'s forward-compatibility section says "L1 adds a per-run or
per-page transform". This design adds neither, on the same grounds the gatefold
question was deferred on: inventing a field on zero evidence is how formats
acquire fields that never get used. The transform is **derived at read time**
from what the reader already has, and a stored transform is reconsidered only
if measurement says the derived one is not good enough. See §4.

Both lines should be corrected when this lands. They are listed in §11.

## What L1 is for

Asked and answered explicitly, because two consumers were available and they
pull the design in different directions.

**Chosen: zoom and pan across a swap.** A reader zooms into a balloon on the
raw, presses the swap, and the same balloon is in front of them in English.
That is the reading experience the whole hub-and-spoke model exists to serve,
and it is the first feature where the alignment is doing something a
side-by-side viewer cannot.

**Not chosen, and still open: travelled note regions.** M5c shipped with a
stated hole — `notes.md`, M5c verification item 5: a note's region tints on the
edition it was written on and draws **zero tints** on the edition it travelled
to, because "drawing the untransformed box would put it over the wrong panel."
That is the same transform this milestone builds. It is deliberately left out
of scope so this milestone stays one shippable thing, and it becomes cheap
afterwards rather than free — the transform is shared, the drawing is not.

## Scope

**In, and built in this plan:**

- Free continuous zoom and pan in single-page and two-page mode.
- A swap that preserves the visible content, mapped through the derived
  transform.
- A page turn that carries zoom and pan and re-clamps.
- The one-off corpus experiment that decides which derived transform is used.
- A `zoom` reader module: preferences, declared keys, generated settings panel.

**Designed here, built later:**

- Travelled note regions, drawn through the same transform.
- A standing gated corpus check, if and only if the experiment's number lands
  close enough that a regression would be invisible (§4).

**Out:**

- Continuous scroll keeps fit-whole. Its zoom is a different mechanism — the
  strip's layout height is a function of scale, and pan competes with scroll
  rather than sitting beside it. Not worth carrying two zoom models across a
  swap in the milestone that invents the first one.
- Any sidecar change. Any `align` change. Any server change.
- L2 text regions.

## 1. Three coordinate spaces, two conversions

The crux of the design, and the part most likely to be got wrong by working
directly in pixels.

| Space | What it is |
| ----- | ---------- |
| **Viewport** | The fixed box the reader looks through. Pixels. |
| **Group** | The drawn unit — one wide tile, or two pages side by side. Fractions. |
| **Image** | Fractions of one page image. What regions already use. |

Zoom and pan state is **one rect in group space**: `{ x, y, w, h }`, the part
of the group the viewport shows. Not a scale factor plus an offset — a rect,
because a rect is what a swap has to preserve and what a clamp has to bound,
and deriving it from a scale and a centre at every use is arithmetic done in
three places instead of one.

Drawing is a single CSS transform on the group container. The individual
`<img>` elements keep their existing fit-whole styling and are not touched.

Two pure conversions, both testable in isolation:

- `imageToGroup(rect, which, layout)` — a rect on one page image, placed in the
  group that draws it.
- `groupToImage(rect, layout)` — the reverse, naming which image and where.

`layout` is what the group already knows: one tile or two, and their aspects.

### Why image space, not slot space

`notes.ts` describes regions as "slot fractions" throughout, but the
implementation measures against the drawn page. `App.tsx`'s `pointOn` says so
exactly — "Where a pointer event falls, **in fractions of the page it is
over**" — and `tiles.ts` records that a joined spread "draws for *both* halves
as a single wide tile and has no one half to name."

So both halves of a joined spread share one coordinate space, and a region on
slot `4R` and a region on slot `4L` are expressed against the same wide image.
This is coherent, but it is not what the doc comments say, and it has a
consequence for L1: **where one edition joins a spread the other splits, the
half-crop is part of the transform**, not something the slot model absorbs on
the way past. The slot model gets the reader to the right *page*; the transform
still has to get them to the right *half of it*.

The doc comments in `notes.ts` that say "slot fractions" should be corrected to
say what the code does. Listed in §11.

## 2. What a swap preserves

**The content, fitted.** The visible rect is mapped through the transform, and
then the viewport is fitted *around* the result — expanded, never shrunk, to
the viewport's aspect.

This is the honest reading of why a reader swaps at all: to see this panel in
the other language. Magnification is not the invariant, because the two
editions are different physical printings at different scan resolutions, and
"the same zoom factor" is a statement about pixels rather than about art.

Aspect ratios do vary between editions, which is measured, not assumed:
`notes.md` records Kaguya's raw at 0.703 against the English 0.667. So the
mapped rect is generally not the viewport's shape, and something has to give.

**Fit beat fill.** Filling the viewport crops a sliver that was visible before
the swap, and a swap that hides something the reader was looking at is the one
failure mode that makes the feature untrustworthy. Fitting shows a little extra
instead, which is the harmless direction — the same conservative choice the
composition loss is described as making elsewhere, where "a reader sees 'no
counterpart' rather than a wrong page."

**Reset-on-swap is kept, as the fallback and as the baseline.** If the
experiment in §4 says no derived transform lands within a panel, resetting to
fit-page on swap is what ships, and it is a preference either way. It is also
the thing the other two candidates are measured against: a transform that does
not beat "throw the zoom away" is not worth having.

## 3. What a page turn preserves

**Both zoom and pan, re-clamped.** The rect is carried unchanged onto the next
group and then clamped into it.

The clamp is not a policy softening the rule — it is forced. A group is two
slots wide or one; a gap draws one; an odd tail draws one. A rect carried from
a two-slot group onto a one-slot group addresses space that does not exist. The
same clamp serves the swap, so there is one function, not two.

*Superseded on 2026-08-07, during implementation.* That paragraph describes a
model in which group space is the **drawn content**, whose width does change
with the slot count. It is not: §1's correction makes group space the
**page-area box**, the same box in every group — so a rect carried across a
turn is already inside it, and the clamp on a turn is defensive rather than
forced. The clamp is still load-bearing, for a different case: `zoomAt` reaching
full size, where the origin is solved from the anchor after the size is capped
and can be any number, while exactly one position exists. Recorded rather than
rewritten, because "the clamp is forced" is the argument that put this in the
design, and it was wrong. Found by Task 7's implementer proving the turn clamp
idempotent and asking why it was there.

**Rejected: keep zoom, pan to the reading-order start.** The conventional
reader behaviour, and it is the wrong one here for the same reason the swap
preserves content: someone zoomed into the bottom-left of a page is usually
working through something, and moving them is a decision made on their behalf.

**Rejected: reset on every turn.** No hidden state, and it makes a reader with
small text re-zoom on every page. Cost is entirely borne by the person the
feature exists for.

## 4. The derived transform, and the experiment that picks it

Two candidates, both computed from what the reader already has — the two
images' natural dimensions — and neither requiring anything new in the sidecar.

1. **Identity.** The same fractions mean the same content: whatever framing
   difference exists between the two is ignored rather than corrected. Right if
   both editions frame the artwork the same way relative to the page, so that a
   point three-fifths down one page is three-fifths down the other whatever
   their aspects.

   *This read "right if both editions frame the whole artwork edge to edge and
   the aspect difference is margin distributed evenly" until 2026-08-07, which
   contradicts itself — edge to edge means there is no margin to distribute.
   Corrected after Task 4's review found the same sentence copied into
   `viewport.ts`.*
2. **Centred letterbox.** The slot with the narrower aspect carries extra
   margin, split evenly between opposing edges; map with a per-axis scale and a
   centred offset.

   As built, this makes the **max** of the two aspects the shared content's
   aspect, which has a consequence worth stating: the horizontal axis is never
   rescaled, and both frames always carry their margin on the vertical axis
   alone. Measured over 2,000 random frame pairs during Task 4's review — `x`
   and `w` came back unchanged to 1e-12 every time.

Candidate 2 is not invented for this design. It is the rule that fixed Tongari
volume 9: `notes.md` records that a letterbox "is centred by construction — the
padding exists to fit an image of one aspect into a frame of another, so an
honest measurement of it is symmetric," and evening opposing pairs up to the
larger took that volume from 98 runs to 7. If that is how framing differences
arise between packagings, the same assumption should map between them.

*Settled on 2026-08-08, by the experiment below. **Neither candidate won**, and
what ships is a third this section never named: `pillarbox`, which takes the
**min** of the two aspects instead of the max — the shared artwork is as tall
as the taller page says, and the wider-aspect page carries its extra as margin
at left and right. Candidate 1 finished second, exactly right on the vertical
and uncorrected on the horizontal. Candidate 2 finished third and lost to doing
nothing: it never rescales the axis that needed correcting and adds four times
identity's median error to the axis that did not.*

*The paragraph above is why candidate 2 was plausible, and it is left standing
because it was sound about **letterboxing** and wrong about **these editions**.
A letterbox really is centred by construction; nothing measured here disputes
that, and the fix to Tongari volume 9 still holds. What the paragraph assumed
without evidence is the sentence beginning "If" — that the framing difference
between two packagings of the same manga is a letterbox at all. The corpus says
it is a pillarbox, in twelve volumes of twelve. The measurement, its control
and the reverse-direction run that falsified the alternative reading are in
`notes.md`, "Which derived transform maps a rect between editions"; the
arithmetic and the figures are in `mapRect`'s doc in `web/src/viewport.ts`. The
sentence in candidate 2 about the max being the shared aspect describes what
shipped between Task 4 and this result, and it measured `x` and `w` coming back
unchanged where the corpus wanted `y` and `h` — a correct observation about the
wrong axis.*

### Why not measure geometry from pixels per pair

Because the corpus has already rejected the two obvious ways twice.
Content-bounding-box autocrop made *every* pair worse — Go-Toubun raw mono
against raw colour fell 99.5% to 91.3% — because the crop box depends on each
page's own content. Per-page trimming made things worse across four series for
the same reason. The lesson recorded there is that **any normalisation that
moves with what a page happens to contain adds more variance than it removes**,
and a per-pair estimated transform is exactly that shape.

Both candidates here are per-*edition-pair* properties computed from framing
alone. That is the shape that worked.

### The experiment

A one-off harness over the corpus, not a standing check — matching how the
rejected autocrop and mirrored-pages hypotheses were settled. For paired slots
across the corpus:

1. Find the best rect-to-rect map between the two drawn images: a coarse grid
   search over per-axis scale and offset, scored with the **existing**
   similarity measure rather than a new one, so the number is in the same
   currency as every other figure in `notes.md` and a bad score cannot be a
   property of a metric invented for this experiment.
2. Report how far each candidate sits from that best map, in fractions of page
   height and width.
3. Report the distribution, not the mean — the tail is the interesting part,
   the same way `margin` is stored as the weakest per-page value in a run.

**The threshold is a panel.** A reader who lands on the right panel has been
served; one who lands on the panel above has not. What fraction of page height
a panel occupies is **a guess pending the measurement** — the working figure is
that a 5% aspect difference on a 1200px page is about 60px, which is a fraction
of a balloon rather than a panel, but the experiment reports the number rather
than assuming it.

**Promotion rule, decided in advance so it is not decided by the result.** If
the residual sits comfortably inside a panel, record the number in `notes.md`
and ship. If it sits near the edge, where a later change could degrade it
invisibly, promote it to a thirteenth `TAIYAKU_CORPUS` check in
`crates/taiyaku-cli/src/corpus.rs`, asserting the bound. If no candidate beats
reset-on-swap, ship reset-on-swap and record why.

*The first branch fired on 2026-08-08: the winner's p90 residual is 2.8% of
page width against the 5% working figure above, so the number is recorded and
there is no thirteenth check. That the threshold itself is still a guess is
unchanged — this experiment did not measure what fraction of a page a panel
occupies either. `notes.md` also records a reason the second branch could not
have been taken as written: the transform is TypeScript, and a Rust corpus
check would re-measure page dimensions no code here can alter and go green
while the browser did something else. The unit tests beside `mapRect` are the
guard.*

## 5. Gestures

`gestureFor` already returns `{ kind: 'ignore' }` unless `press.editing`. So
outside the note editor a drag on the page means nothing today, and pan takes
it wholesale.

| Editor | Drag | Wheel / pinch |
| ------ | ---- | ------------- |
| Closed | Pan | Zoom, anchored at the pointer |
| Open | Unchanged — `gestureFor`'s four cases | Zoom, anchored at the pointer |
| Open, modifier held | Pan | Zoom |

`gestureFor` gains a prepended case and its four existing ones are untouched.
That function is described in its own doc as "the most-changed rule of this
milestone [M5c] and lived where nothing could test it"; rewriting it rather
than extending it would spend that hard-won testability.

Wheel and pinch never collide with a region drag, so zoom needs no
disambiguation in either state.

## 6. Where this touches shipped code

**Corrected on 2026-08-07, while writing the implementation plan.** This
section previously said `paintedBox` was the risk and would gain the viewport
rect as an input. That is wrong, and the truth is better: **`paintedBox` does
not change at all.**

Zoom is one CSS transform on the container holding the group, and
`getBoundingClientRect()` reports post-transform geometry. Both consumers
already measure through it:

- `App.tsx`'s `pointOn` calls `el.getBoundingClientRect()` at event time and
  subtracts `clientX`/`clientY` from it. Under a scaled ancestor the rect is
  the on-screen one and the pointer is in the same space, so the fractions come
  out right with no change.
- `PageAnnotations` measures `img` and `host` rects and stores the difference.
  Same reasoning, same result.

So the arithmetic is already correct, and `pointInSlot`, `regionFromDrag`,
`moveRegion` and `regionContains` are all indifferent as before.

**What does break is staleness, not arithmetic.** `PageAnnotations` re-measures
from a `ResizeObserver`, and a CSS transform changes nothing about layout size,
so the observer never fires on a zoom or a pan. Every marker stays where the
page *was*. The fix is a re-measure trigger — the viewport rect joins the
effect's dependencies — not a new frame of reference.

This holds only for a transform that is pure scale and translate. Nothing here
rotates or skews, and nothing should start.

## 7. The module

A `zoom` `ReaderModule<P>`, declared not self-registering, so the settings
panel is generated from `defaults` with no hand-wiring — the claim M5b makes
and has now proven seven times.

- `defaults` — at minimum: enabled, maximum scale, wheel step, and what a swap
  does (preserve content, or reset).
- `keys` — declared, and checked by `collisions()` at startup so a clash with
  an existing binding is a startup error rather than two modules silently
  fighting. The specific keys are chosen against the live registry during
  implementation, not guessed here.
- Preferences are client-side, via `prefs.ts`, which is where the design puts
  layout and preference: zoom is a fact about the person, not about the work.

## 8. Testing

Matching the existing split — 418 Rust tests, 417 in `web/`.

1. **Pure TypeScript, in `web/`.** `imageToGroup`, `groupToImage`, the two
   transform candidates, fit, and clamp. Each is a function from numbers to
   numbers, which is the level the project already tests its rules at —
   `gestureFor`, `paintedBox` and `regionFromDrag` are the precedent.
2. **The clamp at every boundary.** Two-slot group to one-slot group, one to
   two, into a gap, off the last group, and across a parity break.
3. **The experiment**, §4, run once and written up.
4. **A verification pass against real material**, in the shape M5c's was:
   drive the reader against a corpus work, zoom into a named panel, swap, and
   record where it landed — checked against an independent answer from
   `taiyaku lookup` where the question is which slot, and by eye where the
   question is where on the page.

## 9. Failure modes worth naming

- **The transform is right and the fit is wrong.** Fitting expands the rect, so
  a reader zoomed to the page's edge sees the mapped rect pushed inward by the
  clamp. Correct, and it will look like the transform missed. The verification
  pass should include an edge case deliberately.
- **A swap onto a gap.** `display()` already returns `{ kind: 'gap' }` and the
  page area is replaced by `Gap`. Zoom state is untouched and is restored when
  the reader swaps back, on the same principle that a swap never writes
  position.
- **`naturalWidth` is 0 before decode.** `paintedBox` already falls back to the
  whole box rather than dividing by zero; the transform needs the same guard,
  because a swap can be pressed before the incoming page has decoded. This is
  the one place the transform genuinely cannot be computed, and the honest
  answer there is to leave the rect alone rather than map it through zeros.

- **A stale annotation overlay looks exactly like a wrong transform.** Both
  present as markers a little off the art. §6's re-measure trigger is what
  separates them, and the verification pass should confirm a marker tracks the
  page through a zoom, not merely that it lands correctly once.

## 10. What this deliberately does not answer

Whether a stored per-run transform is ever needed. The experiment can say the
derived transform is good enough for the corpus as it stands; it cannot say it
is good enough for an edition nobody has looked at. `sidecar.md`'s
forward-compatibility rule already permits adding one without a version bump —
a reader must ignore keys it does not recognise — so the door stays open at no
cost, which is the reason not to walk through it now.

## 11. Documentation that goes stale when this lands

- `docs/status.md`, **Later** list — the L1 line, on both counts named in
  "What L1 turned out to be, and where the existing note is wrong".
- `docs/sidecar.md`, forward compatibility — "L1 adds a per-run or per-page
  transform" becomes "L1 may add"; as built it adds neither.
- `web/src/notes.ts` — the doc comments saying regions are in *slot* fractions,
  which the code contradicts and which §1 depends on being clear about.
- `docs/notes.md` — gains the experiment's write-up.
- `web/src/PageView.tsx` — its `fitWhole` comment says upscaling past 100% "is
  a `fit` module setting in M5b and is off until then", which this milestone
  either supersedes or has to reconcile with.

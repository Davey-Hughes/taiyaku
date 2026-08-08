# Fit, the seam, and the keybind help — design

**Status: settled in brainstorming on 2026-08-08. Nothing built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here is a measurement
already recorded in `notes.md`, and a guess says it is one.

Three unrelated reader complaints, raised together after L1 shipped and grouped
onto one branch because each is small and none touches the alignment. They are
not one feature and the document does not pretend otherwise.

## Where they came from

All three came from reading the corpus rather than from a review, which is the
pattern `notes.md` keeps recording: **the browser is where the reader's own
defects surface, and no amount of unit testing substitutes.**

1. "The Japanese raw editions have large gaps and don't fill the page." Not the
   images and not the aspect difference — measured, §1.
2. "Remove the gap between the images in the two-page reader."
3. "Document somewhere what all the keybinds are."

## 1. The `fit` module — pages fill their box

### What was actually wrong

Measured in a browser at a 1400×1263 page area, Go-Toubun v01:

| edition | source | drawn at | fills? |
| ------- | ------ | -------- | ------ |
| `en-mono` | 2147×3056 | 887×1263 | yes |
| `raw-mono` | 685×1024 | **685×1024** | no — 239px of height unused |

`PageView`'s `fitWhole` is `max-width`/`max-height: 100%` with
`width`/`height: auto`. That *downscales* a page larger than its box and leaves
a smaller one at natural size. The raw scans are 685×1024, so on any viewport
taller than about 1024px they sit at 1:1 in the middle of a large letterbox.

**Two things this is not.** It is not the aspect difference — raw 0.6689
against en-mono 0.7026 is about 5%, nowhere near the observed gap. And it is
not raw-versus-English: `en-fan` is 1114×1600 and does the same on a tall
enough display. It is low-resolution scans meeting a viewport bigger than they
are.

The corpus also says the images themselves are clean: the L1 geometry
experiment printed a per-edition flat-border measurement, and `gotoubun-v01`'s
raw reads `0.000, 0.000` — no baked-in bars to trim.

### What ships

A `fit` `ReaderModule` with two settings here, joined by a third in §2:

| Setting | Default | Does |
| ------- | ------- | ---- |
| `upscaleToFit` | **on** | lets a page smaller than its box grow past 100% |
| `upscaleFilter` | `smooth` | interpolation used when upscaling |

`upscaleToFit` on makes `PageView` draw `width: 100%`, `height: 100%`,
`object-fit: contain` — which is the declaration that actually raises the
ceiling, since `object-fit` has nothing to reconcile while box size and content
size coincide. Off restores today's `auto` sizing exactly. `upscaleFilter`
maps `crisp` to `image-rendering: pixelated` and `smooth` to the browser
default.

Both reach the settings panel by registration alone — a boolean infers a
checkbox and a string with declared `options` infers a select, which is the
property `zoomModule` already exercises. No hand-wiring, and nothing in
`Settings.tsx` learns the word "fit".

### The default overturns the M5 design, deliberately

`docs/superpowers/specs/2026-08-02-reader-m5-design.md` specifies this module
and sets `upscaleToFit` **off**, with an argument that is not wrong:

> `upscaleToFit` earns its own setting rather than falling out of "fit": a
> low-resolution scan on a 4K monitor is a real case in this corpus, and the
> alternative is a small page marooned in a large letterbox.

That is the case, described correctly, two milestones before anyone hit it.
What the design could not know is how it would *read*: the first reader to meet
a marooned page reported it as a bug in the images or the aligner, not as a
setting they had not found. A default that makes a working reader look broken
is the wrong default however sound the reasoning behind it.

**The cost is real and stays visible.** Upscaling 685×1024 to fill a 1263px box
is a 23% enlargement of a line-art scan, and the M5 design is right that
bilinear upscaling of inked line work "reads as mush where nearest-neighbour
keeps the line crisp at the cost of visible stair-stepping". That is why
`upscaleFilter` ships alongside rather than later: turning the default on
without exposing the filter would trade one complaint for another. Which
filter is better is a per-scan judgement and stays the reader's.

The M5 design's table must be corrected in place rather than rewritten, per
that file's own convention.

### What is not built

`trim` and `fillWidth`/`fillHeight`, which the M5 design lists in the same
module. Nobody asked for them; `trim`'s argument is about letterboxed scans
(Tongari v09), a different complaint with no reader waiting on it. The module
gains them when someone hits the case, and the M5 design already records the
reasoning for when that happens.

## 2. The seam in two-page mode

### What it is

Measured at the same window: cells 700px wide, images 685px, each centred in
its own cell — **15px of grey at the seam**, and it widens as the window does,
because the cell grows while an un-upscaled image does not. §1 therefore
shrinks this problem on its own; this section makes it exact.

### What ships

Each cell stops centring its page and justifies it **toward the seam** — first
cell right-aligned, second cell left-aligned — with a `gap` between them:

| Setting | Default | Does |
| ------- | ------- | ---- |
| `gap` | `0` | pixels between the two pages in `two` mode |

At `0` the pages touch.

**It joins `fit` rather than `regroup` or a module of its own.** The M5 design
draws the line as "`fit` is geometry and `filters` is pixels", and a seam is
geometry. `regroup` was the other candidate and is the wrong one on inspection:
it owns *which slots pair* — the parity breaks a reader inserts by hand — and
nothing about spacing. A tenth module for a single number would be the third
option and earns nothing; the settings panel groups by module, and "Fit" is
where a reader hunting for page-layout knobs will look.

**Screen position decides which cell aligns which way, not reading direction.**
`cellsFor` has already applied direction by the time the cells are drawn, so
the first cell is always the one on the left. A rule written in terms of `rtl`
here would apply the correction twice.

### The note tabs have to move

A note's tab is drawn at `left: 100%` with a 4px offset — just past the page's
**right** edge, in the letterbox. Close the seam and the left page's tabs land
on the facing page's artwork.

This is exactly what the M5 design was protecting when it wrote "fit-whole
guarantees the side margins the tabs live in on every portrait page". The
guarantee was true of a centred page and stops being true of a seam-aligned
one.

**In `two` mode the tab lives in the margin away from the seam:** the first
cell's tabs move to its left margin, the second cell's stay on its right.
Single and scroll mode are unchanged. `Annotations` gains a side, defaulting to
the current behaviour so no existing call site changes meaning.

Two alternatives it beat. Holding a minimum seam gap open whenever the left
page carries a note keeps one tab rule everywhere, but the gap then appears and
disappears as pages turn, which reads as a layout bug rather than as a
considered space. Letting the tabs overlap is cheapest and is the one thing
this codebase should not do: a tab drawn over the facing page's art asserts a
note is on a page it is not on.

## 3. The keybind help

### The problem with generating it

Module keys are already declarative — each `ReaderModule` lists `{ key, intent }`
and `buildRegistry` checks them for collisions at import. Those can be listed
with no risk of drift.

The built-ins cannot. `Space`, `ArrowRight`, `ArrowLeft`, `Tab`, `c` and `1`–`9`
live inside `intentOf` as a sequence of bare `if` statements, deliberately
answered **before** any module is consulted, and nothing about them is
enumerable. A help screen listing them would be a second copy of a rule, and
this project has now spent nine review rounds on comments that were true when
written and false later.

### What ships

`keys.ts` gains one table that `intentOf` **dispatches from** and the help
**lists from**. A binding that changes behaviour cannot keep an old
description, because there is one description.

Two properties the table must preserve, both load-bearing today:

- **Built-ins are answered before modules.** `intentOf`'s existing comment
  calls this "not so much a priority choice as those keys not being a module's
  to claim". The table is ordered and consulted in order.
- **The digit rule is a range, not nine entries.** `1`–`9` selects the *nth
  edition*, so its help line is one row naming the range, and `0` stays free —
  which is what makes it safe for zoom, as `modules/zoom.ts` already documents.

The overlay opens on `?` **and** `h`, is a `help` `ReaderModule` so it can be
switched off and appears in settings like everything else, and lists two
groups: reading, and per module. Module rows come from the registry; each
module's `intent` string is what is shown, which makes the intent strings
user-facing for the first time and may mean rewording a few.

### Why this is the largest of the three

It touches the reader's only input path. Every key the reader presses goes
through `intentOf`, and `keys.test.ts` is the whole safety net — the table must
be a refactor with the existing tests unchanged, not a rewrite with new ones.
If an existing expectation has to change, that is a signal the table changed
behaviour and the change is wrong.

## Sequencing

One branch, in this order, because the later ones are easier to see once the
earlier ones land:

1. **`fit`** — pages fill their box.
2. **The seam** — worth more once pages actually reach their cell edges, and
   the tab move is easier to verify when the pages are touching.
3. **The help** — independent of both, and the largest.

## Testing

`web/` has no DOM harness, so the split is the one this codebase already uses:
pure rules get Vitest, and everything that is a rendered pixel gets a browser
pass against the corpus, on a scratch sidecar with absolute member paths so
nothing lands in the gitignored `corpus/`.

- Pure and tested: the key table's ordering and its listing; the seam's
  per-cell alignment rule; which margin a tab belongs in.
- Browser pass: a 685×1024 raw filling its box and the filter visibly changing;
  the seam at `0` and at a non-zero gap; tabs in the outer margin with a note on
  each page of a pair; `?` and `h` both opening the help, and every key it
  lists doing what it says.

Go-Toubun v01 is the fixture for all of it: `raw-mono` is the low-resolution
edition at 685×1024, `en-mono` the high-resolution one at 2147×3056, and the
pair makes the seam and the tab sides visible in one screen.

## Documentation that goes stale

- `docs/superpowers/specs/2026-08-02-reader-m5-design.md` — the `fit` table's
  `upscaleToFit` default, corrected in place with the reason.
- `web/src/PageView.tsx` — its `fitWhole` doc names `upscaleToFit` as unbuilt
  and describes the 100% ceiling as unraised. Both stop being true.
- `web/src/Annotations.tsx` — the tab comment quotes the design's "fit-whole
  guarantees the side margins" guarantee, which `two` mode no longer relies on.
- `docs/status.md` — the reader's capabilities, the module count, and the test
  counts.

# The lender that lends nothing, and the arrows that do nothing — design

**Status: settled in brainstorming on 2026-08-13. Built, and shipped on `borrow-lender-and-arrows`.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is the follow-up to `2026-08-12-gap-fallback-design.md`, and it exists
because that design's **§10, "What could not be measured"**, has since been
measured. Both of the questions it left open are answered here, and both
answers came out against what shipped:

> **How often a gap actually arises in real reading**, which is the number that
> says whether this feature is felt or theoretical. It is computable — for each
> ordered pair of editions of a work, count the authority slots that do not
> resolve — but it needs a harness that does not exist and the corpus mounted,
> and no figure here should be quoted for it until one is run.

The harness was written and run on 2026-08-13. The figures are in §1, the
method in §5, and `notes.md` carries the session.

## Where it came from

Nobody set out to revisit `gap-fallback`. The question on the table was which
reader-facing work to do next, and the borrowed page's known discoverability
cost — the arrows are silently inert on it, and `Gap`'s dead-end announcement
went with it — was a candidate whose size nobody knew. §10 above is why it was
unknown, and measuring it was meant to be the cheap step that decided whether
the fix was worth designing.

The measurement decided that, and then found something the design had not
anticipated at all: **the feature mostly does not do what it was built to do.**
That is §2, and it is the larger of the two changes here.

## 1. What was measured

Twelve corpus volumes, every ordered pair of editions, every slot of the
authority — 6,340 reader positions — driven through the production `display`
and `withBorrow` over the WASM-backed `Resolver`, so what is counted is what a
reader would be shown rather than a re-derivation of it.

| | count | share |
| --- | ---: | ---: |
| real page (`at`) | 6,064 | 95.65% |
| borrowed | 276 | 4.35% |
| gap surviving `borrow` | **0** | 0.00% |

**The pooled 4.35% is the wrong summary and is recorded only so that the split
below is not mistaken for the whole.** Borrowing is concentrated:

| reading | borrowed |
| --- | ---: |
| a fan translation (`en-fan` as authority) | 13.42%–21.67% |
| an official edition against another | 0.45%–4.46% |

The worst pair is `gotoubun-v01`, `en-fan → en-mono`: **52 of 240 positions,
21.67%** — roughly one in five. The asymmetry is the point: `en-fan → en-mono`
is 21.67% while `raw-mono → en-fan` is 2.56%, because the authority defines the
slot space and the fan release carries 240 slots against the official volume's
194.

**Zero gaps survived**, which confirms on real material what `borrow.ts`
asserted by reasoning: the authority can always lend, so the only gap that
survives the module is one where the authority's own slot is invalid, and
`restore`/`navigate` already prevent that. That claim is now measured rather
than argued.

### 1.1 Who actually lends

| lender | count | share |
| --- | ---: | ---: |
| the **authority** (the fallback) | **228** | 82.6% |
| a third edition | 48 | 17.4% |

**The candidate list never had more than one entry anywhere in the corpus.**
`lenderFor`'s candidates are `orderedEditions` minus the viewed edition minus
the authority, so a three-edition work leaves exactly one and a two-edition
work leaves none:

| volume shape | candidates | volumes |
| --- | ---: | ---: |
| 2 editions | 0 | 10 |
| 3 editions | 1 | 2 |

So the priority ordering §2 of the previous design reasoned about — prefer a
third edition, and the mirror case it gets wrong — **has never chosen between
two candidates on real material**. It needs four editions to do anything, and
no corpus work has four.

Of the 228 fallbacks, **54 are in the ten two-edition volumes, where the
authority is by construction the edition the reader just left.** The other 174
are in the two three-edition volumes.

### 1.2 The runs an arrow would have to cross

104 unpaired runs across the corpus:

| run length | count |
| --- | ---: |
| 1 position | 64 |
| 2–3 | 28 |
| 4 or more | 12 |
| **longest** | **43** |

The 43-slot run is `gotoubun-v01`, `en-fan → en-mono`. The fan release ships a
44-page `c000` member the official volume has no counterpart for; the run and
that member are not asserted to be the same thing, since spreads split into
slots, but the run is that chapter.

## 2. Fix A: the authority stops lending

```
-  return { edition: state.authority, slot: state.slot }
+  return undefined
```

`withBorrow` already passes a `gap` through unchanged when `lenderFor` answers
`undefined`, so `Gap` draws and no other code moves.

**Why this is close to forced rather than a preference.** `applyIntent`'s
toggle is `state.viewing === state.authority ? previous : state.authority`. On
any borrowed page `viewing !== authority` — otherwise `display` returns the
identity `at` and there is no gap — so **`Space` always lands on the
authority**, which is the fallback's own output. The fallback therefore never
showed the reader anything they could not already reach with one keypress, and
it showed it *degraded*: `at` stays `undefined` on a borrowed page, so there
are no arrows, no annotations, no zoom and no prefetch, and the page that
`Space` would have given properly is instead given as a dead end.

Against that it costs the `Gap` view, which names the missing edition and page
in a sentence and puts both escapes on screen.

**What the reader actually saw, stated as the trace rather than as a
principle.** Reading `en-fan` at a fan-only page, `authority = viewing =
en-fan`. Press `Space` to see the official edition: `swap` leaves the authority
alone, so `viewing = en-mono`, `authority = en-fan`. `en-mono` has no
counterpart — `gap`. `lenderFor` tries `raw-mono`, which has none either,
falls back to the authority, and draws **`en-fan` slot 100: the same pixels
that were on screen before the press**, with a tinted edge and a caption. The
swap appears to do nothing but add a label, and the arrows stop working.

**The cost, stated plainly.** With no third edition to lend, `borrow` becomes
inert: 228 of 276 positions revert to `Gap`, and on the ten two-edition
volumes the module does nothing at all. It becomes a feature of works with
three or more editions, of which the corpus has two. That is a partial
un-shipping of `gap-fallback`'s headline behaviour, taken deliberately, and the
48 surviving borrows are the ones that design was actually argued for — a
genuinely different page the reader could not otherwise see.

**The setting stays.** It still governs those 48, and removing a switch a
reader may have already turned off is a worse answer than leaving one that
does less. Its label and doc change to say what it now does.

**Two doc blocks describe the removed behaviour and must be rewritten rather
than left standing**: `lenderFor`'s "The authority can always lend, which is
why the fallback needs no resolution", and the `borrow` module's own summary.

### 2.1 What was rejected

- **Keep the fallback only where the authority is not one keypress away.** It
  always is — see the toggle rule above — so this collapses to Fix A with more
  code.
- **Draw the authority's page behind a `Gap` overlay.** Two states on one
  screen, and it re-creates the "looks like an ordinary page" problem the mark
  was supposed to solve.
- **Leave it and fix only the arrows.** The measurement says 82.6% of borrows
  carry no information; making their arrows work would make a no-op navigable
  rather than removing it.

## 3. Fix B: the arrows take the offered move

`session.ts::applyIntent`, the `step` case, currently refuses anything that is
not `at`:

```ts
if (shown.kind !== 'at') {
  const within = resolver.slotCount(state.authority)
  if (within === undefined) return { state, previous }
  const { before, after } = nearestPaired(state, resolver, within)
  const offer = intent.by === 1 ? after : before
  if (offer === undefined) return { state, previous }
  return { state: takeOffered(state, offer.slot, resolver), previous }
}
```

**One code path covers both screens, and that is not a shortcut.** `step` calls
the *unwrapped* `display`, so a borrowed page is a `gap` here; the branch
therefore serves `Gap` and `Borrowed` alike, which is what was asked for and
what keeps them from answering the same key differently.

**No direction knowledge enters `session.ts`.** `App.tsx` puts a `step` intent
through `stepFor` before `applyIntent` sees it, so `by: 1` already means "the
next slot in reading order" rather than "the right-hand key". `after` is the
next slot. `modules/direction.ts` remains the one place the physical-to-logical
mapping lives.

**Still inert where there is no offer on that side** — the first run of a
volume has no `before`, and a wholly unpaired edition has neither. `Gap`
already words that case ("No paired page nearby in {edition} either"), and the
key does nothing rather than inventing a destination.

**`takeOffered` re-resolves a slot `nearestPaired` has already resolved.** That
is one redundant `Binding::resolve` per keypress, taken knowingly: it is
exactly what `App.tsx`'s existing `onTake` does, and forking the call shape to
save one resolve on a keypress that only fires on a gap would trade a
consistency the reader can check for a cost nobody has measured as material.
`status.md`'s O14 has the figure for a redundant resolve on a *render* path,
which this is not.

### 3.1 This reverses a decision this project recorded on purpose

`session.ts`'s own doc says:

> there is no slot to add `by` to while showing a gap, so arrows are
> deliberately inert there. The offered moves (`before`/`after`, taken via
> `takeOffered`) are the way out of one, **not a repurposed arrow key**.

That stands as written until this lands, and the reasoning was sound: there is
genuinely no slot to add `by` to. What changed is not the reasoning but the
evidence about its consequence — the borrowed page removed the screen that made
an inert arrow self-explanatory, and 64 of 104 runs are a single position, so
the "offer" and the "next page" are the same destination most of the time.

The doc is rewritten to state the new rule and to record that it replaced this
one. It is not deleted: a reader who finds an arrow doing something on a gap
should be able to find out that it once did not, and why.

**The alternative that lost** was leaving the arrows inert and fixing only the
announcement (§4). It is more conservative and it keeps the recorded decision
intact. It was rejected because it leaves a reader pressing a key that does
nothing on a screen they reach by being lost, and the offers are already the
right destination — the buttons prove it.

## 4. Fix B, second half: the borrowed page says what it is

`Gap`'s offer row is extracted into a component shared by `Gap` and
`Borrowed`, rather than duplicated. `Borrowed` gains one always-visible line —
"No `{viewing}` page here" — above that row.

**Why the mark was not enough.** Its label reads as a caption rather than a
control, which the previous design recorded as the feature's real cost. A
reader who does not press it has nothing on screen telling them why the page
looks ordinary and behaves differently. After Fix A a borrowed page is always a
genuinely different edition's page, so the line is also true in a way it would
not have been for the 228 fallbacks.

The mark keeps revealing the full `Gap` view, and the revealed side keeps its
control back to the borrowed page. Nothing about that interaction changes.

## 5. The measurement, ported to Rust

The harness that produced §1 was a TypeScript test driving the real `display`
and `withBorrow`. It does not land in that form: `web/`'s tsconfig carries no
node types, and every web test to date is filesystem-free by design, so keeping
it would mean adding `@types/node` and the first web test that reads a corpus.

It lands instead as a **gated corpus test in Rust**, beside the existing
twelve, reporting per ordered edition pair: `at`/`borrowed`/`gap`, unpaired run
lengths, and lender source.

**This is a second copy of `display` and `withBorrow`'s roughly twelve lines,
and calling it anything else would be dishonest.** The one-implementation rule
this project holds elsewhere is bent here for a reason worth stating: the
alternative copies a toolchain decision instead, into the gate that CI runs.
The copy is small, its inputs are a `Sidecar` rather than a `Resolver`, and it
is exercised against figures this document pins — so a divergence shows up as
the numbers moving, which is the only check that would catch it either way.

**It asserts almost nothing.** A threshold on a corpus figure is a check that
fails when the corpus changes, which `notes.md` has already ruled is not a
measurement. The structural invariants are asserted — every position lands in
exactly one arm, and after Fix A the surviving-gap count is no longer zero — and
the numbers print for a person to read.

## 6. Testing

- `lenderFor` answers `undefined` where no third edition resolves, and still
  answers a third edition where one does.
- `withBorrow` passes the gap through on `undefined`, by identity.
- `applyIntent` takes the `after` offer on `by: 1` and the `before` offer on
  `by: -1`; stays put when that side has no offer; and is unchanged on `at`.
- A test that the intent reaching `applyIntent` is already rtl-mapped, so a
  later refactor cannot quietly move `stepFor` and invert the arrows on a gap
  only.
- Component tests for the shared offer row in both hosts, and for `Borrowed`'s
  new line.
- The corpus test as the regression guard on §1's figures.

## 7. Branch shape

Three stacked PRs, because the remote is squash-only and a branch should carry
one checkable claim:

1. **The measurement.** No behaviour change; it is the evidence for the other
   two and should be reviewable without them.
2. **Fix A.** One line, plus the two doc blocks and the `status.md` entry.
3. **Fix B.** `applyIntent`, the shared offer row, `Borrowed`'s line, and the
   `session.ts` doc that this reverses.

## 8. What is deliberately not done

- **No per-work lender order.** Still deferred, and §1.1 strengthens the case
  for deferring: the ordering has never had two candidates to choose between.
- **No borrowed page as a reading position.** `at` stays `undefined`;
  annotations, zoom and prefetch are untouched. Fix B gives the arrows a
  destination without making the page a slot.
- **No change to `Gap`'s offers themselves**, only to who can reach them.
- **No new keybinding.** Both fixes reuse `←`/`→`.
- **No server change.**

## 9. What could not be measured

**Whether a reader wants the 48 surviving borrows.** The measurement says how
often the state arises and who lends; it cannot say whether a page from a third
edition is welcome. Only reading answers that, and no reading has.

**How often the reader is two or more swaps from the authority**, which is the
one case where the removed fallback carried information — the authority's page
would not then be the page they just left. The sweep enumerated edition
*pairs*, not reader trajectories, so this is unmeasured. It is bounded above by
the 174 three-edition fallbacks and is zero in the other ten volumes.

**Whether four or more editions would exercise the lender ordering sensibly.**
No corpus work has four. `gotoubun-v01` has unused `en-color` and `raw-color`
directories and could be built into a five-edition sidecar to find out; that is
a measurement, not a fix, and it is not part of this work.

**Pages that exist only in a non-authority edition remain unreachable**, and
this design does not change that. Reading `en-mono`, the fan release's extra
pages have no `en-mono` slot to stand on, so no arrow and no borrow reaches
them; making them the authority is the only route. Noted because it was raised
while brainstorming and could easily be mistaken for something Fix B addresses.

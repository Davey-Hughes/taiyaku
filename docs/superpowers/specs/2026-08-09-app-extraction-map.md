# The `App.tsx` extraction map — design

**Status: settled from reading the file on 2026-08-09, revised the same day
against a review that re-read the source. Built, bar one part deliberately
skipped.** `hooks.ts`, `correction.ts`, `CorrectionBanner.tsx`, `GroupView.tsx`,
`swapView.ts`, `startup.ts`, `keyplan.ts`, `module.ts`, `modules/zoom.ts` and
`modules/prefetch.ts` are all in `web/src` today. **Task 13a's `PageArea.tsx`
was not built**, and was not forgotten either: the block it names is 167 lines
of JSX holding **five** executable statements — three in one `onPointerCancel`
handler, one `.map` body and one `.filter` predicate — so extracting it would
buy a file and the ~20 props counted below while moving almost no branching
logic out of `App()`. Judged not worth the churn, which is a judgement about
what the line bound was measuring and is the same argument §0 makes. Task 13a's
other half, `hooks.ts`, shipped.

*This header said "Nothing built" until 2026-08-10. That was true when the
document landed and stopped being true as the extractions shipped; the line
was simply never revisited. §6's residue table below is left exactly as it
stands — it already labels its unmeasured rows as unmeasured.*

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine unless it says otherwise, and a guess says it is one.

### What the review overturned

Two claims in the first draft were wrong in a way that changed the plan, and
both are corrected in place below rather than footnoted, because a design
document read later should say what is true rather than what it once thought.

1. **The 412 retry in `writeNotes` does not work today, and §4.1 was silently
   fixing it.** The recursive call re-sends the stale validator from the render
   closure, so the merge never lands — and the outer call then tells the reader
   it did. §4.1 now names the defect, and Task 11 fixes it deliberately, with a
   regression test written before the fix and watched failing against a faithful
   port of today's behaviour.
2. **The six browser tests are not the detector for §4.6's reconciliation
   invariant.** `PageView.browser.test.tsx` imports neither `App` nor
   `renderGroup`; every one of its six tests mounts a hand-written JSX replica
   built from bare `PageView`s, so nothing in it changes when `renderGroup`
   becomes `GroupView`. The invariant has **no** detector today. §4.6 now
   specifies one that runs in `jsdom`, and the "Task 12 must be run locally"
   constraint the first draft put in §5 and §7 is gone, because it never existed.

Everything else here is the first draft's reading, kept.

The coverage design (`2026-08-09-coverage-gate-design.md`, §6) deliberately
refused to name module boundaries, on the grounds that inventing them from a
`grep` of hook counts would be exactly the kind of guess that document exists to
forbid. It named a starting hypothesis — *correction and history; decode and
prefetch orchestration; session and progress persistence; notes* — and said in
so many words that it was a hypothesis. This document is that decision made
properly: every `useState`, `useRef` and `useEffect` inventoried from the file,
grouped by what the inventory shows actually moves together, and sequenced so
each extraction lands on its own.

**The hypothesis survived in two of its four parts and was corrected in the
other two.** §3 records which, and what the inventory said instead.

---

## 0. What was measured before anything was proposed

### The hook counts in the brief are `grep -c` counts, not call sites

| counted | `useState` | `useRef` | `useEffect` |
| ------- | ---------- | -------- | ----------- |
| `grep -c` on the whole file | 33 | 33 | 21 |
| actual call sites | **26** | **30** | **20** |

The difference is prose: `App.tsx` discusses its own hooks in comments —
`layout.ts`'s "was a `useState` until 2026-08-08", `swapQueue`'s "lazily created
once via `useState`'s initializer", `writeProgressRef`'s three-sentence argument
about `useRef` not being lazy. Every design document downstream of the brief
carries 33/33/21; the real figures are 26/30/20 and the inventories below are
complete at those counts.

Nothing turns on the difference. It is recorded because this branch exists
because a coverage number described something other than what a reader thought
it did, and a hook count that counts its own documentation is the same mistake
in miniature.

### The file, by line

Measured over `App()`, lines 334–3359 — 3,026 lines:

| | lines | share |
| - | ----- | ----- |
| comment | **1,538** | 50.8% |
| code | 1,406 | 46.5% |
| blank | 82 | 2.7% |

**`App()` is more than half documentation, and this governs §6.** A line budget
applied to this function measures prose as much as logic, and the residue check
has to be read with that in front of it.

The whole file is 3,360 lines with five top-level declarations; lines 1–333 are
178 lines of imports, six constants, two helpers (`centreOf`, `frameOf`), two
interfaces (`SidecarShape`, `Session`), `allSlots`, and `App()`'s own doc.

### The coverage, re-measured

From `web/coverage/coverage-summary.json` as it stands on this branch:

| | statements | covered | pct |
| - | ---------- | ------- | --- |
| `App.tsx` | **737** | **0** | 0% |
| whole `web/src` tree | 1,957 | 945 | 48.28% |

1,012 statements uncovered across the tree, of which `App.tsx` holds 737 —
**72.8%**. (The brief says 1,015 and 73%; the three-statement drift is between
two runs of the same command, not a disagreement.)

The next largest uncovered file is `Annotations.tsx` at 67. `App.tsx` is eleven
times the second-largest problem.

### Why none of it is reachable today

`App.tsx:139–145` states the constraint and `viewport.ts:320` restates it from
the other side: *"`App.tsx` value-imports the resolver and is unreachable from
Vitest, so a rule left inline there is covered by typecheck alone."* A test that
imports `App.tsx` imports `resolver.ts` as a value, whose `taiyaku-wasm` import
only resolves once the gitignored `crates/taiyaku-wasm/pkg/` has been built — so
such a test passes on a developer's machine and breaks CI and every fresh clone.

Task 14 lifts this for `App.tsx` itself with `vi.mock('./resolver')`. **It does
not lift it for the extracted modules**, and §4's first rule is written to keep
it that way.

---

## 1. Step 2 — the state inventory

56 entries: 26 `useState`, 30 `useRef`. "Moves with" names the other state that
is written in the same handler or effect, which is the evidence §4 groups on.
Read/write sites were found mechanically (every non-comment line mentioning the
identifier, bucketed by which block of the file it falls in) and then read.

### The 26 `useState`

| # | line | name | holds | written by | read by | moves with |
| - | ---- | ---- | ----- | ---------- | ------- | ---------- |
| 1 | 335 | `session` | `Session \| undefined` — resolver, editions, alignments, `ReadingState`, `previous` | load effect; keydown reader-intent; `take`; `goToSlot`; `confirmCorrection` | 8 effects, the render body, `sessionRef` | `etag` (both set at load and on a correction) |
| 2 | 351 | `view` | `Rect` over the drawn group; invariant `w === h` | keydown step (`clampRect`); 3 zoom key cases; wheel effect; swap effect; `trackRegion` pan | `pageLongEdge`, `Viewport`, `PageAnnotations.version` | `drawnRef`, `swapRef`, `panRef` |
| 3 | 354 | `chromeVisible` | `boolean` | keydown `Tab`; keydown `c` (forces `true`) | `Chrome` | nothing — except the one `c` coupling |
| 4 | 358 | `settingsVisible` | `boolean` | keydown `s` | `Settings` | nothing |
| 5 | 362 | `creditsVisible` | `boolean` | keydown `C`; `CreditsPanel.onGo` | `CreditsPanel` | nothing |
| 6 | 365 | `helpVisible` | `boolean` | keydown `?`/`h`; `Escape`; `Help.onClose` | `Help`, `helpVisibleRef` | nothing |
| 7 | 373 | `pending` | `Pending \| undefined` — the correction being asserted | keydown `c`; `Escape`; `pickCounterpart`; `confirmCorrection`; Cancel | correction derivations, `Chrome`, the banner, `pendingRef` | `correctionNotice` (cleared at every site), `submitting`, `etag` |
| 8 | 388 | `submitting` | `boolean` — a correction is on the wire | `confirmCorrection` only (`try`/`finally`) | the banner's `disabled`, `submittingRef` | `pending` |
| 9 | 400 | `etag` | sidecar validator | load effect; `confirmCorrection` (success **and** 412 re-read) | `confirmCorrection`'s guard | `session` |
| 10 | 408 | `correctionNotice` | `string \| undefined` | every site that writes `pending` | the banner | `pending` |
| 11 | 418 | `notesDoc` | `NotesDoc \| undefined` | load effect; `writeNotes` (success and 412 re-read) | `allNotes`, `NotePanel.doc` | `notesEtag`, `notesHistory` |
| 12 | 419 | `notesEtag` | notes validator | load effect; `writeNotes` | `writeNotes`'s guard | `notesDoc` |
| 13 | 424 | `notesBusy` | `boolean` — a note write is on the wire | `writeNotes` only | `NotePanel.busy`, `notesBusyRef` | `notesDoc` |
| 14 | 428 | `notesHistory` | `History<Note[]>` | load effect (seed); `writeNotes` (`record`); `stepTo` | `canUndo`/`canRedo`, `stepTo` | `notesDoc` |
| 15 | 431 | `notesNotice` | `string \| undefined` | `writeNotes` sets it; **seven** sites clear it | `NotePanel.notice` | set with `notesDoc`, cleared with `panelMode` |
| 16 | 435 | `panelMode` | `'edit' \| 'review' \| undefined` | keydown `n`/`a`; `openNote`; `closeEditor`; delete; `NotePanel.onClose`/`onGo` | overlay prefs, cursor, `draftImage`, `NotePanel`, `panelModeRef` | `edits`, `currentId`, `notesNotice` |
| 17 | 446 | `edits` | `Record<string, Note>` — every note this session touched | `editCurrent`; keydown `n`; `openNote`; `closeEditor`; `stepTo`; `beginRegion` (move); `endRegion`; delete | `draft`, `editedNotes`, `applyEdits` | `currentId`, `panelMode` |
| 18 | 447 | `currentId` | which of `edits` the panel shows | same eight sites as `edits` | `draft`, `hereBehind`, scroll overlays | `edits`, `panelMode` |
| 19 | 484 | `prefsMap` | `Record<string, object>` — every module's slice | persist effect (reads); keydown `d`/`b`/`r`; `setMode`; `handleSettingChange`; `handleSettingReset` | 10 `sliceFor` reads, `Settings`, `Help` | nothing outside itself |
| 20 | 568 | `swapQueue` | `DecodeQueue` for the live turn | its own rebuild effect | every mounted `PageView` | `prefetchQueue` (same trigger) |
| 21 | 575 | `prefetchQueue` | `DecodeQueue` for the prefetch window | its own rebuild effect | prefetch effect | `swapQueue` |
| 22 | 589 | `pageCohort` | `Cohort` — never rebuilt, no setter | — | every mounted `PageView` | `swapQueue` |
| 23 | 601 | `drawnSrcs` | `{ first?: string; second?: string }` — what the two cells actually painted | `showFirst`/`showSecond`, from `PageView.onShown` | `renderGroup` **only** | nothing |
| 24 | 727 | `viewportWidth` | `number` | its own resize effect | `notesPlacement`, `pageLongEdge` | nothing |
| 25 | 777 | `dragging` | `Region \| undefined` — the live rectangle | `trackRegion`, `endRegion`, `onPointerCancel` | both `PageAnnotations` | `dragRef` |
| 26 | 1427 | *(discarded)*, `tick` | `number`, value unused | 30s interval | nothing — it exists to re-render | `turnsRef`, `sessionStartRef` |

### The 30 `useRef`

Six exist to defeat one staleness: the keydown effect subscribes on
`[session?.resolver, session?.editions]` and so its closure is frozen at load.
Seven more are assigned **during render** because they are derived state bound
below `if (!session) return null`, where no hook may go.

| # | line | name | holds | written | read | why a ref |
| - | ---- | ---- | ----- | ------- | ---- | --------- |
| 1 | 391 | `submittingRef` | mirror of `submitting` | effect | keydown | closure staleness |
| 2 | 466 | `panelModeRef` | mirror of `panelMode` | effect | keydown | closure staleness |
| 3 | 470 | `notesBusyRef` | mirror of `notesBusy` | effect | keydown | closure staleness |
| 4 | 616 | `sessionRef` | mirror of `session` | effect | keydown, prefetch effect | closure staleness |
| 5 | 624 | `pendingRef` | mirror of `pending` | effect | keydown `Escape` | closure staleness |
| 6 | 635 | `helpVisibleRef` | mirror of `helpVisible` | effect | keydown `Escape` | closure staleness |
| 7 | 645 | `groupedRef` | `Group[]` on screen | render (2137) | keydown step, prefetch effect | derived, bound below the guard |
| 8 | 652 | `alignedBreaksRef` | `DerivedBreaksCache` | `alignedBreaksCached` mutates it | render | a `useMemo` cannot sit below the guard |
| 9 | 659 | `creditsPrefsRef` | `CreditsPrefs` | render (2138) | keydown `C` | derived, bound below the guard |
| 10 | 662 | `readingRef` | `Direction` in effect | render (2145) | keydown step (`stepFor`) | derived, bound below the guard |
| 11 | 670 | `zoomPrefsRef` | `ZoomPrefs` | render (2146) | keydown zoom cases, `recordSwap` | derived, bound below the guard |
| 12 | 677 | `helpPrefsRef` | `HelpPrefs` | render (2147) | keydown `?`/`h` | derived, bound below the guard |
| 13 | 695 | `drawnRef` | `{ url, cell, half?, hug } \| undefined` | render (2382) | keydown zoom, wheel, swap effect, `beginRegion` | derived, bound below the guard |
| 14 | 712 | `pageAreaRef` | the page-area `<div>` | React | wheel effect, swap effect, `recordSwap` | a DOM handle |
| 15 | 719 | `swapRef` | `{ from: Drawn; edition } \| undefined` | `recordSwap`; swap effect clears | swap effect | crosses a render boundary |
| 16 | 743 | `hereRef` | `NoteHere[]` on the page | render (2230) | keydown `n` (`nextColour`) | derived, bound below the guard |
| 17 | 747 | `stepBackRef` | `() => void` | render (2038) | keydown `z` | a live closure for a frozen listener |
| 18 | 748 | `stepForwardRef` | `() => void` | render (2039) | keydown `Z` | as above |
| 19 | 749 | `closeEditorRef` | `() => void` | render (2040) | keydown `Escape` | as above |
| 20 | 750 | `saveNotesRef` | the notes debounce | render (1297) | `closeEditor` | as above |
| 21 | 752 | `dragRef` | `{ x, y, el, intent, from?, noteId? } \| null` | `beginRegion`, `endRegion`, cancel | `trackRegion`, `endRegion` | must not re-render per frame |
| 22 | 773 | `panRef` | `{ x, y } \| null` — the previous pan frame | `beginRegion`, `trackRegion`, `endRegion`, cancel | `trackRegion` | must not re-render per frame |
| 23 | 1267 | `writeProgressRef` | `Debounced<[string, Position]>` | first render only | progress + flush effects | one timer for the component's life |
| 24 | 1290 | `writeNotesRef` | `Debounced<[Note[]]>` | first render only | `scheduleSave`, `stepTo`, delete, close, flush | as above |
| 25 | 1302 | `writeNotesLatestRef` | `(notes) => Promise<void>` | render (1896) | the debounce's own callback | the debounce holds its first callback forever |
| 26 | 1359 | `restored` | `boolean` | progress effect | progress effect | skip exactly the load's own run |
| 27 | 1403 | `turnsRef` | `number[]` of turn timestamps | turns effect | `pagesPerMinute` at render | client-only, must not re-render |
| 28 | 1404 | `restoredTurn` | `boolean` | turns effect | turns effect | skip the load as a "turn" |
| 29 | 1417 | `sessionStartRef` | `Date.now()` at mount | never | `timeLine` at render | fixed for the page's life |
| 30 | 1449 | `currentGroupRef` | the current group's `<div>` | React | scroll effect | a DOM handle |

**Undo and redo are `z` and `Z`, not `u` and `U`.** Rows 17–18 said `u`/`U` in
the first draft, which is the vi binding and not this reader's:
`modules/annotations.ts:78–79` binds `UNDO_INTENT` to `z` and `REDO_INTENT` to
`Z`, beside `n`, `r` and `a`. Corrected here and in §4.1, and named because a
map that gets a keybinding wrong sends the implementer looking for a case that
is not there.

### Two name collisions the implementer will hit

- **`pending`** is the correction state (373) *and* a local `const pending =
  swapRef.current` inside the swap effect (1651). They are unrelated.
- **`current`** is `history.ts`'s `current` imported as `currentHistory`, a local
  `const current = sessionRef.current` in four keydown cases, and the drawn
  group `const current = grouped[currentIndex]` at 2160.

Both are the kind of thing a careless move silently gets wrong.

---

## 2. Step 3 — the effect inventory

All 20, in file order.

| # | lines | dependencies | body | subscribes / schedules | cleanup |
| - | ----- | ------------ | ---- | ---------------------- | ------- |
| 1 | 392–394 | `[submitting]` | `submittingRef.current = submitting` | — | none |
| 2 | 467–469 | `[panelMode]` | mirror | — | none |
| 3 | 471–473 | `[notesBusy]` | mirror | — | none |
| 4 | 502–506 | `[prefsMap]` | `savePrefs` for every registered module | `localStorage` | none |
| 5 | 571–573 | `[prefetchPrefs.maxConcurrent]` | rebuild `swapQueue` | — | none — in-flight jobs finish under the old cap |
| 6 | 578–580 | `[prefetchPrefs.maxConcurrent]` | rebuild `prefetchQueue` | — | none |
| 7 | 617–619 | `[session]` | mirror | — | none |
| 8 | 625–627 | `[pending]` | mirror | — | none |
| 9 | 636–638 | `[helpVisible]` | mirror | — | none |
| 10 | 730–734 | `[]` | `setViewportWidth(window.innerWidth)` | `window` `resize` | removes the listener |
| 11 | 779–841 | `[]` | `listWorks` → `getSidecarWithEtag` → `JSON.parse` → `orderedEditions` → `wasmResolver` → `getProgress` → `getNotes` (own `catch`) → `restore` → six setters | five network reads | `cancelled = true`, checked before every write |
| 12 | 850–1255 | `[session?.resolver, session?.editions]` — `biome-ignore`d; the handler reads current state through `setSession`'s updater and six mirror refs | the whole keydown dispatcher, 413 lines, plus `recordSwap` | `window` `keydown` | removes the listener |
| 13 | 1319–1338 | `[writeProgress, saveNotes]` | flush both debounces | `document` `visibilitychange` **and** `window` `pagehide` | removes both |
| 14 | 1361–1376 | `[session?.work, session?.state.authority, session?.state.slot, writeProgress]` — `biome-ignore`d; adding `session` would write on every swap | debounced `putProgress`, skipping the load's own run via `restored` | the debounce timer | none |
| 15 | 1406–1411 | `[session?.state.authority, session?.state.slot]` — `biome-ignore`d, same reason | `turnsRef.current = nextTurns(...)` | — | none |
| 16 | 1428–1431 | `[]` | `tick((t) => t + 1)` | `setInterval`, 30 s | `clearInterval` |
| 17 | 1451–1459 | `[mode, session?.state.{authority,slot,viewing}]` — `biome-ignore`d; the deps are triggers, the body reads only `mode` and a ref | `currentGroupRef.current?.scrollIntoView` in scroll mode | — | none |
| 18 | 1482–1547 | `[mode, session?.state.{authority,slot,viewing}, prefetchPrefs.{enabled,ahead,behind}, prefetchQueue]` — `biome-ignore`d | `groupWindow` → one `decodeTo` per target through `prefetchQueue`, each re-checking `superseded()` when the queue reaches it | queued decodes | none — supersession is checked at run time instead |
| 19 | 1576–1630 | `[mode, zoomPrefs, loaded]` — `biome-ignore`d; `loaded` is a mount trigger | non-passive wheel → `wheelBy` → `zoomAt` about the pointer | `pageArea` `wheel`, `{ passive: false }` | removes the listener |
| 20 | 1650–1730 | `[session?.state.{authority,slot,viewing}, zoomPrefs.onSwap]` — `biome-ignore`d; `mode` is deliberately **absent** (its cleanup would drop the swap rather than redo it) | applies a carried swap through `mapView`, or waits for a capture-phase `load` | `pageArea` `load`, capture phase | removes the listener **and** clears `swapRef` if no newer swap replaced it |

### What the effect inventory decides

- **Nine of the twenty are one-line plumbing** (#1–3, 5–9, 16). They hold no
  decision. Nothing is gained by unit-testing them and nothing is lost by
  leaving them; §6 uses them to shrink the residue, not §4 to grow a module.
- **Six carry a `biome-ignore` whose comment is the argument for the dependency
  array itself** (#12, 14, 15, 17, 18, 19, 20 — seven, counting #12's). That
  reasoning is about the array, so it stays with the array wherever the array
  goes. It is 150-odd lines and §6 counts it.
- **Three do real work with real branches** (#11 load, #12 keydown, #20 swap)
  and a fourth is close (#18 prefetch). Between them they are the extraction.

---

## 3. Step 4a — what the inventory said about the hypothesis

The coverage design's four-part hypothesis, checked against §1 and §2:

**"Correction and history" — rejected.** They are two units that never move
together. `pending`/`submitting`/`etag`/`correctionNotice` are written by five
sites, none of which touches `notesHistory`; `notesHistory` is written by three
sites (load seed, `writeNotes`'s `record`, `stepTo`), none of which touches
`pending`. History moves with the **notes document**, not with correction — it
is the undo stack for notes and nothing else in the reader has one. Pairing them
would have put `history.ts`'s consumer in a file about alignment.

**"Decode and prefetch orchestration" — confirmed, and smaller than it sounds.**
`swapQueue`/`prefetchQueue`/`pageCohort` and their two rebuild effects are 53
lines holding one decision (rebuild on `maxConcurrent`). The *real* work in this
area is the prefetch effect's window-and-supersede logic, which belongs to
`modules/prefetch.ts` where `groupWindow` already lives.

**"Session and progress persistence" — confirmed.** `session` and `etag` are set
together at load and on a correction; the progress write is guarded by
`restored`; both the progress and turns effects carry the same load-bearing
dependency-array argument. One unit.

**"Notes" — confirmed, but it is three units, not one.** The inventory separates
them cleanly: `notesDoc`/`notesEtag`/`notesBusy` are the **stored document and
its writes**; `notesHistory` is the **undo stack**; `panelMode`/`edits`/
`currentId` are the **editing session**. `notesNotice` straddles — set by the
write machine, cleared by seven editing transitions.

*The first draft said two units and put them in one new file, on the grounds that
they are one screen. §4.1 revises that.* Being one screen is a fact about the
reader's experience and not about where a rule is tested, and §4's third rule —
a pure synchronous rule extends its existing noun module — sends the editing
session to `notes.ts` and the undo stack to `history.ts`, both of which exist,
are pure, and are already covered. Only the write, which takes ports and is
`async`, earns a file of its own.

**And one unit the hypothesis did not name at all: the view.** `view`,
`drawnRef`, `swapRef`, `panRef`, `pageAreaRef` and `zoomPrefsRef` are written by
**seven** sites — `setView` at 1132, 1140, 1147, 1195, 1610, 1705 and 2654,
across two effects, four key branches and one pointer handler — and read by
nothing else. *(The first draft said six and left 1195 out of both the module
list and the "stays" lists. It stays: `setView(clampRect)` on a turn is a
one-line call into `viewport.ts`'s existing `clampRect`, and the seventeen lines
of comment above it argue about the call's idempotence, so both stay in App with
the residual `reading` dispatch. It is the one `setView` site that neither §4.2
nor §4.3 takes.)* It is 143 code lines, every one of them arithmetic, and it is
the single most testable thing in the file. Missing it is the cost of grouping
by `useState` name rather than by write site — `view` is one `useState` and a
quarter of the file's geometry.

---

## 4. Step 4 — the modules

### Two rules decide where anything goes

**Rule 1: nothing may import `resolver.ts` as a value.** Every extracted module
takes `Resolver` as a *type* and `wasmResolver` as an injected port. This is not
a preference — `App.tsx:139–145` records that a test importing a module that
value-imports `resolver.ts` passes locally and breaks CI and every fresh clone.
`pairing.ts` and `notes.ts` already follow it (`import type { Resolver }`), and
`fixtures.ts`'s `fakeResolver` is the seam their tests use.

**Rule 2: a pure module beats a custom hook, and the reason is coverage, not
taste.** A hook's branches can only be reached by mounting a component, which is
exactly what the coverage design rejected — *"a failure would point at three
thousand lines rather than at a unit."* Moving branching logic into a hook
relabels the uncovered statements rather than covering them. So: **branching
logic goes in a pure module; a hook is only ever allowed to hold plumbing with
no branch worth naming.** §6 is where the one exception is spent.

A third rule falls out of reading the neighbours: **a pure synchronous rule
extends its existing noun module; a port-taking asynchronous workflow gets a new
module named for the workflow.** `modules/direction.ts` owns `stepFor`,
`modules/regroup.ts` owns `toggleEditionBreak`, `modules/credits.ts` owns
`creditSlotsIn` — each is a rule App used to inline, living with the preference
that drives it. That is the pattern for the extensions below. It is not the
pattern for `loadSession`, which does five network reads and would put the only
`async` in a synchronous file.

---

### 4.1 The note editor — **three units, and one of them is a defect**

**The first draft made this one new file, `editing.ts`, taking about 560 lines.
That was wrong twice over**, and the review that found it is why this section is
now the longest in the document.

Wrong first about the boundary. The `Draft` reducer and the region rules
(`pressTakesPan`, `regionUnderDrag`, `anchorFrom`, `GRAB_SLOP`) are pure
synchronous rules over `Note` and `Region` — which is precisely the third rule
this section opened with, *a pure synchronous rule extends its existing noun
module*. Their noun module exists and is `notes.ts`, which already holds
`gestureFor`, `regionFromDrag`, `moveRegion`, `mergeOver`, `paintedBox`,
`pointInSlot` and (since the `Annotations.tsx` task earlier on this branch)
`artOffset`. `stepHistory` is the same shape over `History<T>` and belongs in
`history.ts`, which is already generic. **Only `saveNotes` earns a new file**: it
is the one part that takes ports and is `async`, and putting the only `async` in
`notes.ts` is what §4's third rule declines for `loadSession`.

What that costs: three destinations to review instead of one, and a `notes.ts`
that grows from 674 lines (measured) to somewhere around 900 (a guess — most of
the 560 is comment that will not all survive the move intact). What it buys: no
new file receives 560 lines, the two files that do receive most of them are
already at 93–100% coverage with tests to extend rather than invent, and the
largest genuinely *new* surface in Task 11 is about a hundred lines.

*If `notes.ts` at 900 lines reads as too large when the implementer gets there,
the split to reach for is by subject and not by size — the region geometry
(`paintedBox`, `pointInSlot`, `artOffset`, `regionFromDrag`, `moveRegion`,
`regionContains`, `pointAt`, `isPoint`) is a coherent half that could become
`region.ts`. That is a decision for somebody looking at the file, and this map
does not make it in advance.*

Wrong second about the write, which is the rest of this section.

| unit | lands in | holds |
| ---- | -------- | ----- |
| the draft, and the region rules | `web/src/notes.ts` *(extended)* | `PanelMode`, `Draft`, the eight transitions, `sameNotes`, `pressTakesPan`, `regionUnderDrag`, `anchorFrom`, `GRAB_SLOP` |
| the undo step | `web/src/history.ts` *(extended)* | `stepHistory` |
| the write, and its 412 | `web/src/notesWrite.ts` *(new)* | `NotesPorts`, `SaveOutcome`, `saveNotes` |

**Moves in.** Every range below was re-checked against the file for this
revision; the first draft's list had four ranges that did not match the surface
they were justifying, and those are marked.

| range | becomes | lands in |
| ----- | ------- | -------- |
| 191–203 | `sameNotes` | `notes.ts` |
| 205–213 | `GRAB_SLOP` | `notes.ts` |
| 448 | `shownNote` — the derivation `draft` is today | `notes.ts` |
| 452–462 | `changeShown` *(named `editCurrent` in the file; the first draft gave the range under the old name and the export under the new one, and never said they were the same thing)* | `notes.ts` |
| 1028–1071 (`n`) | `newNote` | `notes.ts` |
| 1082–1089 (`z`/`Z`) *(the first draft said `u`/`U` — see §1)* | the `stepHistory` call | `history.ts` |
| 1090–1098 (`a`) | `toggleReview` | `notes.ts` |
| 1907–1910 and 3295–3305 | `nothingOpen` / `NOTHING_OPEN` — the same three-setter transition written out twice, which is half the argument for a reducer | `notes.ts` |
| 1918–1978 | `saveNotes` | `notesWrite.ts` |
| 2028–2035 | `stepHistory` | `history.ts` |
| 2211–2221 | `withEdits`, `visibleNotes` | `notes.ts` |
| 2255–2273 | `openNote` (2259–2273); `applyEdits` (2255–2257) becomes a call to `editsToWrite`, which `notes.ts` already exports and already tests | `notes.ts` |
| 2510–2521 | `regionUnderDrag` | `notes.ts` |
| 2592–2596 | `pressTakesPan` | `notes.ts` |
| 2623–2645 | `grabNote` | `notes.ts` |
| 2700–2714 | `anchorFrom`, `anchorNote` | `notes.ts` |
| 3281–3293 | `afterDelete` *(the first draft exported `afterDelete` and gave it no range at all; it is the `NotePanel` `onDelete` handler, which is in the JSX rather than in the note-editing block)* | `notes.ts` |

**Four corrections to the first draft's ranges**, all of them cases where a
range and a surface disagreed:

- **1277–1304 does not move.** The first draft listed it as "the notes debounce
  doc" moving in, while also listing "the debounce refs" under *stays behind*.
  Both cannot be true, and the range is the refs: 1277–1289 is
  `writeNotesRef`'s doc, 1290–1297 is the ref, the `if`, and `saveNotes`, and
  1298–1304 is `writeNotesLatestRef` and the comment explaining why it exists.
  It is all wiring and all of it stays.
- **2195–2242 splits.** 2211–2221 (`editedNotes`, `notesForHere`) moves, as
  `withEdits` and `visibleNotes`. 2222–2242 — `here`, `hereBehind`, `hereRef`
  and `scrolledNotes` — **stays**: they are one call each into `notes.ts`'s
  already-tested `notesHere` and `notesBySlot`, plus one `filter` and one ref
  assignment. The first draft claimed the whole range and exported nothing for
  the second half.
- **`entries` gets no export.** The first draft gave 2042–2050 a range and no
  surface. 2048 is `notesDoc?.notes ?? []` and 2049–2050 is a ternary around
  `reviewList`, which `notes.ts` already exports and covers. Both stay as one
  line each.
- **1072–1081 is not here.** The `r` case writes `annotationsPrefs.reveal` and
  is a preferences write; it belongs to §4.10 and lands in Task 13. §5's Task 11
  claimed it as well, which made it the one range in the map assigned to two
  tasks at once.

**Surface, on `notes.ts`:**

```ts
/**
 * Which surface the note panel is showing, or none.
 *
 * Declared here rather than in `NotePanel.tsx`, which re-exports it: a module
 * that must not import React cannot take its own state's type from a `.tsx`,
 * even type-only, without making the dependency graph read backwards.
 */
export type PanelMode = 'edit' | 'review'

/** Everything the editor holds that is not the stored document. */
export interface Draft {
  panel: PanelMode | undefined
  /** Every note this editing session has changed, keyed by id. */
  edits: Record<string, Note>
  currentId: string | undefined
}

export const NOTHING_OPEN: Draft

/** The note the panel is showing. Derived, never stored — see `App.tsx:448`. */
export function shownNote(draft: Draft): Note | undefined

/** `n`: always a brand-new note, however many the slot already holds. */
export function newNote(
  draft: Draft,
  at: { edition: string; slot: number },
  here: NoteHere[],
  id: string,
  now: string,
): Draft

/** A margin tab or a review row. Whatever is in `edits` wins over `stored`. */
export function openNote(draft: Draft, id: string, stored: Note[]): Draft

/** Change the note on show, in place. The same `Draft` back when there is none. */
export function changeShown(draft: Draft, change: (note: Note) => Note): Draft

/** A region drag grabbed a note: bring it in and show it. */
export function grabNote(draft: Draft, note: Note): Draft

/** Give a note a new anchor — a swept rectangle or a planted point. */
export function anchorNote(
  draft: Draft,
  id: string,
  region: Region,
  now: string,
): Draft

/** `a`: show the review list, or close it if that is what is open. */
export function toggleReview(draft: Draft): Draft

/** Escape, a background click, the panel's own close, a delete. */
export function nothingOpen(): Draft
export function afterDelete(draft: Draft, id: string): Draft

/** Stored notes with this session's edits over them, plus the ones it invented. */
export function withEdits(stored: Note[], edits: Record<string, Note>): Note[]

/** Those, filtered to one edition when `travel` is off — filtered before resolving. */
export function visibleNotes(
  stored: Note[],
  edits: Record<string, Note>,
  travel: boolean,
  viewing: string,
): Note[]

/** Whether two documents are the same, for history's sake — by content. */
export function sameNotes(a: Note[], b: Note[]): boolean

// ---- the region gesture ----------------------------------------------------

export interface DragStart {
  x: number
  y: number
  el: HTMLImageElement
  intent: 'draw' | 'move'
  from?: Region
  noteId?: string
}

/**
 * Pan, or hand the press to `gestureFor` below.
 *
 * `zoomable` is **one flag, not two**, and it is `modules/zoom.ts`'s
 * `zoomable(prefs, drawn)` — the same predicate the three zoom keys are gated
 * on (App.tsx:1126, 1139, 1146) and the same one this press is gated on
 * (2593–2594). The first draft had `zoomEnabled` and `drawn` as separate
 * fields here and left the identical pair inline in the keydown switch, which
 * would have put one rule in two places across two tasks — and the pan this
 * gates lands in `modules/zoom.ts` in Task 12 while this lands in `notes.ts`
 * in Task 11, so "two places" would also have meant "two weeks apart".
 *
 * The split across two modules is deliberate and is the price of both
 * boundaries being right: the *decision* to pan is part of what a press on the
 * page means, which is this file's subject, and the *arithmetic* of a pan
 * frame is the rect's own move, which is `zoom.ts`'s. Sharing `zoomable` is
 * what keeps them from drifting.
 */
export function pressTakesPan(press: {
  zoomable: boolean
  onControl: boolean
  editing: boolean
  shift: boolean
}): boolean

/** What the region should be, given where the drag began and where it is now. */
export function regionUnderDrag(
  intent: 'draw' | 'move',
  from: { x: number; y: number },
  to: { x: number; y: number },
  started?: Region,
): Region

/** A click rather than a drag, while drawing, plants a point instead. */
export function anchorFrom(
  intent: 'draw' | 'move',
  region: Region,
  start: { x: number; y: number },
): Region

/** How near a marker counts as grabbing it, in page fractions. */
export const GRAB_SLOP: number
```

**Surface added on `history.ts`:**

```ts
/**
 * A step, and the value it has to write. `undefined` where it is a no-op —
 * `undo` and `redo` hand back the *same* history object at either end, which
 * is what App.tsx:2029 tests, so identity is the no-op rule rather than a
 * bounds check restated here.
 *
 * Generic, like everything else in this file. The reader has exactly one undo
 * stack and it is the notes', but the rule is about `History<T>` and writing
 * it as `History<Note[]>` would be the only thing in this module that knew
 * what it was holding.
 */
export function stepHistory<T>(
  from: History<T>,
  to: History<T>,
): { history: History<T>; write: T } | undefined
```

---

#### 4.1a The 412 retry does not work, and this branch fixes it

**This is the most important section in this document.**

`App.tsx:1935–1977`. `writeNotes` recurses at 1962 to retry a merged write after
a stale validator. The recursive call re-reads `notesEtag` at 1939 and 1942 —
**from the render closure**, which `setNotesEtag(fresh.etag)` at 1959 does not
update, because a `useState` setter does not change the `const` the current
render bound. So the retry sends the validator that just 412'd, 412s again, and
this time `retry` is `false`, so it falls past 1955 to `setNotesNotice(message)`
at 1971 and returns **normally** — a refusal that does not throw.

Control then resumes in the outer call at 1963, which overwrites that notice
with:

> These notes changed on another device; yours were kept on top.

**The merge never lands, and the reader is told it did.** Their notes are not on
top of anything; the other device's document is what the server still holds, and
the only trace is a `console.error` that never fires because nothing threw. This
was verified against the source for this revision, not inferred from the shape.

Two things kept it invisible. `App.tsx` is at 0% coverage (§0), so no test runs
this path at all. And the path needs a real concurrent writer to reach, which no
manual pass has ever set up.

**The decision: fix it on this branch, in Task 11, with the regression test
written first.** The alternative was to port the defect faithfully into
`notesWrite.ts` and open an O-number against it, which is what the first draft
did by accident — its `saveNotes` doc says "on a stale validator re-read, lay
`notes` back over what arrived by id, and write that", which is the *intended*
behaviour and not the shipped one, so the extraction would have quietly repaired
the bug with no test naming it. That was rejected for the reason this whole
branch exists: a fix nobody can point at is indistinguishable from a fix that did
not happen, and this one changes what a reader's data does.

The cost of fixing here rather than later: Task 11 stops being purely a move, and
its diff has to be read as two changes rather than one. The sequence below is
what keeps them separable.

**The sequence Task 11 must follow, in this order:**

1. **Extract `saveNotes` as a faithful port of today's behaviour** — stale
   validator and all. The retry re-sends the `etag` argument rather than the one
   the re-read returned, and the `merged` outcome is produced whether or not the
   second write landed. Commit this, or at least stage it, as its own step: the
   whole point is that there is a moment where the module is today's code in a
   new file.
2. **Write the tests below, encoding the *correct* semantics** — the retry uses
   the fresh validator; the "kept on top" notice appears only where the retry
   actually succeeded.
3. **Run them against the faithful port and watch cases 2, 3 and 4 fail.** That
   is the regression test. It *demonstrates* the defect rather than asserting it,
   and the failure output is the artefact the Task 11 report quotes.
4. **Fix `saveNotes`, and watch them pass.** Nothing else changes in the same
   commit.

**The test cases. Ten, and which of them fail at step 3 is the point.**

| # | given | expect | at step 3 |
| - | ----- | ------ | --------- |
| 1 | `put` resolves | `{ kind: 'written' }` with the returned doc and etag; `get` never called | passes |
| 2 | `put` 412s; `get` returns a doc and a **different** etag; second `put` resolves | `puts[1].etag === get`'s etag — asserted on the recorded argument, not on the outcome | **fails** — the port sends `puts[0].etag` again |
| 3 | `put` 412s; `get` succeeds; second `put` 412s too | `{ kind: 'refused' }` carrying the *second* failure's message | **fails** — the port answers `merged` with the "kept on top" notice |
| 4 | as 2 | `{ kind: 'merged' }`, `notice` exactly "These notes changed on another device; yours were kept on top.", and `doc`/`etag` the ones the **second** `put` returned | **fails** — the port's second write never succeeds, so it carries the re-read's document and validator instead |
| 5 | `put` 412s; `get` rejects | `{ kind: 'refused' }` with the *original* 412 message, and `error` set to the re-read's failure for the caller to log | passes — a control, pinning today's `console.error` path |
| 6 | `put` 412s twice | `put` called exactly twice, `get` exactly once | passes — a control, pinning "retried once and once only" (App.tsx:1931–1933) across the fix |
| 7 | as 2, with `notes` holding an id the re-read's document does not, and the re-read holding an id `notes` does not | the second `put`'s document is `mergeOver(fresh.doc.notes, notes)` — both ids present, the shared id taken from `notes` | passes — a control, proving the fix changes the validator and not the merge rule |
| 8 | `put` rejects with a message *not* containing "changed since you read them" | `{ kind: 'refused' }` with that message; `get` never called | passes |
| 9 | `put` 412s; `get` resolves with **no** etag | `{ kind: 'refused' }` — there is no validator to write against, so the conditional write cannot be made | new: today's code never reaches this, because it reads the closure's etag instead. Decided here rather than left to the implementer: refusing is what the load path already does when the notes read fails (`Opened.notesEtag` absent, §4.7), and inventing an unconditional write would be the only place in this reader that writes without one |
| 10 | as 2, with `doc.notes` deliberately different from the `notes` argument | `doc` supplies only `work`; the content sent is `notes` on the first attempt and `mergeOver(fresh.doc.notes, notes)` on the second, never `doc.notes` | passes — a control. Today's `writeNotes` sends `{ work: notesDoc.work, notes }` (1942–1945) and this is the easiest thing in the port to get wrong by "tidying" the two parameters into one |

The "changed since you read them" match is by substring, deliberately, and stays
that way: `api.ts` only promises the status is somewhere in the thrown message,
and `routes.rs`'s `put_notes` is where the wording comes from. The test fixtures
must throw the server's real sentence rather than a made-up one, or case 8 tests
nothing.

**What makes the cases expressible.** Everything above is an assertion about
*which arguments the ports received* and *which outcome came back* — no DOM, no
component, no timers. `NotesPorts` is the seam, and a test's `put` is three
lines that push `{ work, etag, doc }` onto an array and resolve or reject. That
is the whole reason this is a pure module with injected ports rather than a hook:
cases 2, 6, 7 and 10 are assertions on an argument or a call count, and there is
no way to make any of them from outside a mounted component.

Three of the ten fail at step 3 — 2, 3 and 4 — and the other seven are controls
that must pass both before and after, because the fix is meant to change the
validator the retry uses and nothing else.

**Surface, `web/src/notesWrite.ts` *(new)*:**

```ts
import type { FetchedNotes } from './api'
import type { Note, NotesDoc } from './notes'

/**
 * `api.ts`'s `putNotes` and `getNotes`, injected. Rule 1 does not apply here —
 * `api.ts` value-imports nothing from `resolver.ts` — but they are ports
 * anyway, because half of §4.1a's cases are assertions about the arguments a
 * write received, and there is no other way to make one.
 */
export interface NotesPorts {
  put(work: string, etag: string, doc: NotesDoc): Promise<FetchedNotes>
  get(work: string): Promise<FetchedNotes>
}

export type SaveOutcome =
  /** The write landed on the first attempt. */
  | { kind: 'written'; doc: NotesDoc; etag: string | undefined }
  /**
   * A stale validator, re-read, merged, and **written** — this variant is
   * produced only where the second write itself succeeded. `doc` and `etag`
   * are the second write's, not the re-read's.
   */
  | {
      kind: 'merged'
      doc: NotesDoc
      etag: string | undefined
      /** "These notes changed on another device; yours were kept on top." */
      notice: string
    }
  /**
   * Nothing landed. `notice` is what the reader is shown; `error` carries a
   * failed re-read for the caller to log, the shape §4.8's `stale` uses for
   * the same reason.
   */
  | { kind: 'refused'; notice: string; error?: unknown }

/**
 * Write the document, and on a stale validator re-read, lay `notes` back over
 * what arrived by id, and write that. Retried once and once only.
 *
 * **The retry writes against the validator the re-read returned.** Stated here
 * because the version this replaced did not: it re-sent the validator that had
 * just been refused, so the second write always failed too, and the caller
 * reported the merge as having landed anyway. `App.tsx:1935–1977`, and §4.1a
 * for the trace and the ten cases that hold this shut.
 */
export async function saveNotes(
  work: string,
  etag: string,
  doc: NotesDoc,
  notes: Note[],
  ports: NotesPorts,
): Promise<SaveOutcome>
```

---

**Stays behind:** the eight `useState`, `notesBusyRef`/`panelModeRef` and their
mirrors, `dragRef`/`panRef`/`dragging`, the debounce refs (1257–1304, all of
them — see the corrections above), the `!notesDoc || notesEtag === undefined`
guard at 1939 (which is why `saveNotes` takes `etag: string` and not
`string | undefined`), and two ten-line DOM readers — `draftImage` (which `<img>`
a gesture may act on, matched by `src`) and `pointOn` (`getBoundingClientRect`
into `paintedBox`/`pointInSlot`). Those two read the DOM and cannot be pure;
everything they read *for* is above.

**One behaviour difference beyond the fix, and it must be recorded in the Task 11
report.** Today `writeNotes` sets `notesBusy` false at 1961, immediately before
its recursive retry sets it true again at 1940. With `saveNotes` as one call, App
sets busy once around the whole thing and that pair of updates does not happen.

**The first draft justified this as removing a visible flicker. That
justification was wrong, and the correction makes the change safer rather than
more dangerous.** Under React 19 (`react@^19.2.8`, `web/package.json:21`) all
updates are batched automatically, including those made in a promise
continuation. `setNotesBusy(false)` at 1961 and the recursive `setNotesBusy(true)`
at 1940 are separated by no `await`: 1962's `await` suspends *after* the
recursive call has run its synchronous prologue, which is the guard at 1939 and
the setter at 1940. The two updates are therefore in one continuation and React
commits neither separately, so **no un-busy frame is ever rendered** and there is
very likely nothing observable to lose at all.

Which leaves the difference as a fact about the code rather than about the
screen, and it is still disclosed, for two reasons. It is a fact somebody could
check and find changed. And the batching argument is a claim about React's
scheduler, not a measurement — if the implementer can cheaply prove it by
counting renders in the Task 11 tests, that is worth more than this paragraph.

The alternative was an `onBusy` port threaded through `saveNotes` purely to
reproduce the pair of updates. Rejected: it would add a port to reproduce
something the runtime already discards, and if it ever *did* become visible it
would be visible as the flicker `notesBusy`'s own doc (App.tsx:420–423) says must
not happen — *"an exit taken mid-write would tell the reader nothing had happened
while it was still going to land."*

---

### 4.2 `web/src/modules/zoom.ts` — the rect's own moves *(extended)*

Every function below is driven by `ZoomPrefs`. `enabled` gates all of them,
`step`/`maxScale` drive two, `onSwap` drives the last. That is the evidence they
belong here rather than in a new file: `modules/direction.ts` owns `stepFor` for
exactly this reason.

**Moves in:** 218–252 (`CONTENT_MAP` and its 28-line measurement, `centreOf`),
1117–1149 (the three zoom key cases), 1594–1626 (the wheel's anchor arithmetic
and zero-box guard), 2649–2678 (`trackRegion`'s pan step, including the
`Object.is` bail-out).

**Surface added:**

```ts
import type { MapKind, Rect } from '../viewport'

/** A rect's own centre — where a keyboard zoom anchors, having no pointer. */
export function centreOf(rect: Rect): { x: number; y: number }

/** A box, as much of a `DOMRect` as any of this needs. */
export interface Box {
  left: number
  top: number
  width: number
  height: number
}

/**
 * Where a client point falls over the group, in the rect's own fractions.
 * `undefined` for a zero-sized box — an element mid-teardown, not a viewport;
 * dividing by it puts `NaN` into the rect and the page vanishes with no error.
 */
export function anchorIn(
  rect: Rect,
  box: Box,
  at: { x: number; y: number },
): { x: number; y: number } | undefined

/**
 * Whether the rect can be moved at all right now: the module is on **and**
 * something is drawn to move it over.
 *
 * One predicate, because it is asked in four places that must agree — the
 * three zoom keys (App.tsx:1126, 1139, 1146) and the pan press (2593–2594).
 * `drawn` rather than `mode !== 'scroll'` for the reason `beginRegion`'s own
 * comment gives: `drawnRef` is already `undefined` in scroll mode *and* in a
 * gap, where `Gap` renders inside the page area and would otherwise pan a rect
 * nobody is drawing.
 *
 * This is `Guards.zoomable` in §4.11 and `pressTakesPan`'s `zoomable` in §4.1,
 * and it is the one thing shared between Task 11's boundary and Task 12's.
 */
export function zoomable(prefs: ZoomPrefs, drawn: boolean): boolean

/**
 * `=`, `-`, `0`. The rect back unchanged where the module is off.
 *
 * The `enabled` check here is **not** made redundant by `zoomable` above: from
 * the keydown path `keyPlan` has already refused the key, but `wheelStep`
 * shares this function's arithmetic and the wheel listener is subscribed
 * whatever `enabled` says (its dependency array carries `zoomPrefs` whole).
 * Belt and braces on purpose, and named so nobody deletes it as dead.
 */
export function zoomStep(
  rect: Rect,
  which: 'in' | 'out' | 'whole',
  prefs: ZoomPrefs,
): Rect

/** A wheel notch or a trackpad pinch. `undefined` where the box is unusable. */
export function wheelStep(
  rect: Rect,
  wheel: { deltaY: number; deltaMode: number; ctrlKey: boolean },
  at: { x: number; y: number },
  box: Box,
  prefs: ZoomPrefs,
): Rect | undefined

/**
 * One pan frame. The *same* rect back where the pan had nowhere to go, so
 * React's `Object.is` bail-out applies — see App.tsx:2665–2673 for the
 * re-render this saves at fit-whole, which is where a reader who never zooms
 * spends the whole session.
 */
export function panStep(
  rect: Rect,
  box: { width: number; height: number },
  from: { x: number; y: number },
  to: { x: number; y: number },
): Rect

/** Which derived transform a swap maps through. */
export function swapMapKind(prefs: ZoomPrefs): MapKind
```

**Stays behind:** `view` itself, `panRef`, the two DOM subscriptions — the
non-passive `wheel` listener and its `{ passive: false }` argument, whose
55-line justification is about the subscription and so stays with it — and the
seventh `setView` site, `setView(clampRect)` at 1195 with its seventeen lines of
idempotence argument, which §3 explains.

---

### 4.3 `web/src/swapView.ts` — carrying the view across a swap *(new)*

The two halves of a swap are available at two different moments and neither can
wait for the other. That two-phase machine is 99 lines with five early returns,
all uncovered, and it is where a wrong answer displaces the view by half a group
and carries the displacement forward through every later swap.

Not folded into `viewport.ts`: that file is what a rect *means* (`mapRect`'s doc
alone is 60 lines of measurement). This is when to ask it. Not folded into
`modules/zoom.ts` either: it takes no `ZoomPrefs` beyond the `MapKind` §4.2
already derives, and it is the one part of the geometry that is a state machine
rather than arithmetic.

**Moves in:** 254–276 (`frameOf`), 714–721 (`swapRef`'s shape), 1221–1252
(`recordSwap`), 1632–1730 (the apply/wait/drop decision).

**Surface:**

```ts
import type { Cell, Drawn, Frame, MapKind, Rect } from './viewport'
import type { ArtworkHug } from './modules/fit'

/** As much of an `<img>` as measuring needs, so no DOM is required to test it. */
export interface Measurable {
  src: string
  naturalWidth: number
  naturalHeight: number
}

/**
 * The natural size of the page `url` names, from the images on screen now.
 * `undefined` covers three states that are one state for a caller: not on
 * screen, on screen but not decoded, decoded to nothing.
 */
export function frameOf(
  images: Iterable<Measurable>,
  url: string,
): Frame | undefined

/** What the page area is drawing — `drawnRef`'s shape, named. */
export interface DrawnPage {
  url: string
  cell: Cell
  half?: 'L' | 'R'
  hug: ArtworkHug
}

/** A swap whose incoming page has not been measured yet. */
export interface CarriedSwap {
  from: Drawn
  /** The edition being **left** — what tells a no-op digit from a real swap. */
  edition: string
}

/**
 * Measure the outgoing page, at the last moment it is still on screen.
 * `undefined` means this swap carries nothing, which the reader sees as the
 * view left where it was — the design's §9 answer, reached the same way for a
 * swap onto a gap and for one pressed faster than a page decodes.
 */
export function carrySwap(
  drawn: DrawnPage | undefined,
  frame: Frame | undefined,
  leaving: string | undefined,
  enabled: boolean,
): CarriedSwap | undefined

/**
 * **Four kinds, not three.** The first draft had `apply`/`wait`/`drop`, which
 * conflated two pairs of distinct cases in today's code — and one of those
 * conflations would have introduced a defect, so the four are enumerated here
 * against the lines they come from.
 */
export type SwapStep =
  | {
      kind: 'apply'
      /** Hand these to `mapView` inside `setView`'s updater. */
      from: Drawn
      to: Drawn
      areaAspect: number
      mapKind: MapKind
    }
  /**
   * Keep the `load` listener attached; do **not** clear the carry.
   *
   * `why` because the two waits are not the same wait, and only one of them
   * ends by itself:
   *
   * - `'undecoded'` (App.tsx:1698, `!frame`) — the incoming image is on screen
   *   but has not decoded, so `frameOf` finds nothing to measure. A `load`
   *   event is exactly what ends this, which is why the listener is there.
   * - `'no-box'` (1703, `box.width <= 0 || box.height <= 0`) — the page area
   *   has no layout. **No `load` will end this**, because nothing about the
   *   image is what is missing; it ends on the next render that gives the area
   *   a box, and the effect re-runs on the reader's next move. The first draft
   *   gave this case no rule at all. It is a wait rather than a drop because
   *   that is what today's `return false` does, and because a zero box is an
   *   element mid-teardown — the same guard the wheel handler states in full,
   *   where dividing by it puts `NaN` into the rect and the page vanishes with
   *   no error.
   */
  | { kind: 'wait'; why: 'undecoded' | 'no-box' }
  /**
   * Nothing to land on, or a swap that changed nothing. **Clear the carry.**
   * Three sites: `!host || !drawnRef.current` and
   * `viewing === carried.edition` (1664–1671), and `!to` (1693).
   */
  | { kind: 'drop' }
  /**
   * Finished, and the carry belongs to somebody else — **do not clear it.**
   *
   * App.tsx:1676, `swapRef.current !== pending`: a later swap replaced this one
   * while it waited, and that newer swap has its own effect with its own
   * listener. Today's code returns `true` here *without* touching `swapRef`,
   * and the difference from `drop` is load-bearing: clearing would throw away
   * the newer swap's carry and leave the view where it was, which is the
   * displacement `swapView.ts` exists to prevent — and it would do it only
   * under a swap pressed faster than a page decodes, which is the hardest
   * state in this file to reach by hand.
   *
   * The first draft folded this into `drop`. That is the defect the fourth
   * kind exists to keep out.
   */
  | { kind: 'done' }

/**
 * `stillCurrent` is `swapRef.current === carried` read at the call site. It is
 * a parameter rather than a check the caller makes before calling, so the
 * "finished but do not clear" rule is inside the tested module rather than in
 * the four lines of App that survive it.
 */
export function swapStep(
  carried: CarriedSwap,
  stillCurrent: boolean,
  to: DrawnPage | undefined,
  frame: Frame | undefined,
  box: { width: number; height: number } | undefined,
  viewing: string | undefined,
  mapKind: MapKind,
): SwapStep
```

**The caller's whole job, after this:** `apply` → `swapRef.current = undefined`
then `setView(rect => mapView(...))`; `drop` → `swapRef.current = undefined`;
`done` → nothing; `wait` → nothing, and keep listening. Four arms, no
arithmetic, and the one thing it must not do is treat `done` as `drop`.

**Stays behind:** `swapRef` and `pageAreaRef`, the capture-phase `load`
subscription, and the cleanup that clears `swapRef` only when no newer swap has
replaced it. The dependency-array comment — in particular why `mode` is
deliberately absent — stays with the array.

---

### 4.4 `web/src/modules/prefetch.ts` — the window, and what supersedes it *(extended)*

**Moves in:** 1482–1547's body, minus the effect skeleton.

```ts
import type { Display } from '../reading'
import type { Group } from '../pairing'
import type { Position, Resolver } from '../resolver'

/** One page to warm: the slot, and the page index `pageUrl` actually takes. */
export interface PrefetchTarget {
  slot: number
  page: number
}

/**
 * The pages either side of `at` worth warming. Empty in scroll mode, while
 * disabled, or in a gap — all three for reasons `prefetchModule`'s own doc
 * gives, and all three answered here rather than by three guards in a caller.
 */
export function prefetchTargets(
  grouping: Group[] | undefined,
  at: Display,
  prefs: PrefetchPrefs,
  mode: Mode,
  slotAt: (edition: string, index: number) => Position | undefined,
): PrefetchTarget[]

/**
 * Whether a window built for `queuedFor` still matters. Strict equality on the
 * slot, not proximity: the position the reader moved *to* has queued its own
 * window, so nothing is lost by dropping the old one.
 */
export function superseded(
  queuedFor: { edition: string; slot: number },
  now: Display | undefined,
): boolean
```

`slotAt` rather than a `Resolver` so a test hands it a function; `Resolver`
stays available as a type for the caller.

**Stays behind:** the effect, its dependency array and that array's comment, and
the `prefetchQueue.run(...)` loop.

---

### 4.5 `web/src/pairing.ts` — the second cell *(extended, two functions)*

**Moves in:** 2758–2777 — the `wide`-versus-`alone` distinction and the
measurement behind it.

```ts
/**
 * Whether the second cell is showing a real second page or a mirror of the
 * first — read off what is **drawn**, not off the target.
 *
 * `wide` is a fact about the page and comes from `cellsFor`. `alone` governs
 * whether a cell is *visible* and follows the pixels: state moves on the
 * keypress and the pictures wait for their decode, so keying this on the
 * target is what made the layout flap — measured at three changes in 114 ms
 * during a flick, with the artwork never changing at all.
 */
export function secondCellAlone(
  target: Cells,
  drawn: { first?: string; second?: string },
): boolean
```

---

### 4.6 `web/src/GroupView.tsx` — one group, drawn *(new component)*

`renderGroup` is 173 lines drawing one group in every mode. It is the largest
piece of JSX in the file with a boundary anybody can name, and it lands in the
`jsdom` project beside the other eleven components.

**Moves in:** 2717–2889.

**`drawnSrcs` (591–608) does *not* move, and the first draft was wrong to move
it.** It said `renderGroup` "owns `drawnSrcs` entirely — that state is written by
two `PageView` callbacks it passes and read by nothing else in the file", and
each clause of that is true. What it misses is what `useState` in a component
means: **component-local state is lost on unmount**, and this state's whole
purpose is to survive a moment when the target and the pixels disagree.

The two `renderGroup` call sites are in different arms of the same ternary —
3069, inside scroll mode's per-group `<div>`, and 3111, inside `<Viewport>`. A
reader going two → scroll → two therefore unmounts and remounts everything at
that position. As `App` state, `drawnSrcs` survives that. As `GroupView` state it
comes back `{}`, `drawnBoth` is false, and `alone` falls back to `targetAlone`
for as long as it takes both cells to report — **which is the predicted value,
keying the layout on the target, which is the exact thing this state was
introduced to stop.** Measured, before that fix: three layout changes in 114 ms
during a flick with the artwork never changing at all. And by Critical 2 above,
nothing in the test suite would have seen it.

So: `drawnSrcs`, `showFirst` and `showSecond` stay in `App`, and `GroupView`
takes `drawn` and `onShown` as props. The alternative — moving the state and
disclosing the regression — was rejected because a component extraction that
reintroduces a measured flicker is not an extraction, and because the disclosure
would have had no test behind it.

*Named and not fixed here:* App-level `drawnSrcs` is itself **stale** across a
scroll excursion — it holds whatever the last two-page group drew, which may be
a different group entirely by the time the reader comes back. That is a real
question about which of stale-populated and empty is the better wrong answer, and
it is not this map's to settle. The jsdom test below pins today's behaviour so
that a later answer has to be a deliberate change.

```tsx
export interface GroupViewProps {
  group: Group
  mode: Mode
  reading: Direction
  src: (slot: number) => string
  alt: (slot: number) => string
  hug: (group: Group, slot: number) => ArtworkHug
  buffered: boolean
  decodeQueue: DecodeQueue
  cohort: Cohort
  fit: FitPrefs
  /** What the two cells have actually painted. `App` state — see above. */
  drawn: { first?: string; second?: string }
  /** `PageView.onShown`, routed back to `App`'s `showFirst`/`showSecond`. */
  onShown: (cell: 'first' | 'second', src: string) => void
}

export function GroupView(props: GroupViewProps): ReactElement
```

**The reconciliation invariant this must not break.** `cellsFor`'s doc and the
comments at 2738–2751 and 2815–2844 record that changing the *element type* at
this position forces a remount and throws away `PageView`'s decode gate. Putting
`<GroupView>` where `<PageView>`/`<div>` used to be changes the type once, at the
extraction; after that it is the same type on every render and the children
reconcile by position exactly as they do now.

**The first draft said the six browser tests were the only thing that could see
this. They cannot see it at all.** `PageView.browser.test.tsx` imports `act`,
`createCohort`, `createDecodeQueue`, `PageView` and `mount` — **no `App` and no
`renderGroup`** — and every one of its six tests mounts a hand-written JSX
replica built from bare `PageView`s. Nothing in that file changes when
`renderGroup` becomes `GroupView`; the six pass identically before and after,
which makes them worthless as a detector and, worse, made the first draft's
"they must pass before and after" read as evidence when it was a tautology. The
invariant has **no detector today.**

**So Task 12 writes one, in `jsdom`:**

> Mount `GroupView` in `two` mode with a paired group. Capture the two `<img>`
> nodes. Re-render the same `GroupView` with a **wide** group (a joined spread,
> where `cellsFor` gives both cells one URL) and then with a paired group again.
> Assert the same two `<img>` node objects are still in the tree — `===` on the
> element references, not on their attributes.

Node identity is what a remount destroys and a prop update preserves, and asking
about it needs no layout engine, no real decode and no `getBoundingClientRect` —
so `jsdom` answers it, which is what makes it part of the gate CI runs (the
coverage design's §5 argument, applied to a correctness test rather than a
coverage number).

**It must be seen to fail before it is trusted**, for the reason §4.1a demands of
the 412 test. The deliberate regression is the shape the two-page branch had
before the flicker fix, and the comment at 2738–2751 describes it exactly: make
`GroupView` return a bare `<PageView>` for a wide group instead of the collapsed
second cell. Watch the test fail on the wide→paired step, put it back.

Two more jsdom tests belong with it, and they are cheap once the first exists:

- **`alone` follows the pixels.** With `drawn` empty, the second cell's
  visibility follows `cellsFor`'s `alone`; with `drawn` holding two identical
  sources it is hidden whatever the target says; with two different sources it is
  visible whatever the target says. This is `secondCellAlone` (§4.5) seen through
  the component, and it is the flicker fix's own assertion.
- **A freshly mounted `GroupView` still draws what `drawn` says.** Mount it for
  the first time with a populated `drawn` whose two sources are *different* and a
  target group that is `alone`, and the second cell is visible. This is the test
  that pins the decision above: it is only passable if the answer comes from the
  prop, so it fails the moment somebody moves the state back inside the
  component, where a fresh mount has nothing.

---

### 4.7 `web/src/startup.ts` — opening a work *(new)*

**Moves in:** 278–318 (`SidecarShape`, `Session`, `allSlots`), 779–841's body.

```ts
import type { FetchedNotes, FetchedSidecar, StoredProgress } from './api'
import type { Direction } from './modules/direction'
import type { NotesDoc } from './notes'
import type { ReadingState } from './reading'
import type { Alignment } from './session'
import type { Position, Resolver } from './resolver'

/** Enough of the sidecar to pick a first edition, a direction, and a verdict. */
export interface SidecarShape {
  work: { reading: Direction }
  editions: Record<string, unknown>
  hub: string
  alignments: Alignment[]
}

export interface Session {
  work: string
  resolver: Resolver
  reading: Direction
  editions: string[]
  alignments: Alignment[]
  state: ReadingState
  previous: string
}

export interface StartupPorts {
  works(): Promise<string[]>
  sidecar(work: string): Promise<FetchedSidecar>
  /** `wasmResolver`, injected — see Rule 1. Never imported here. */
  resolve(json: string): Promise<Resolver>
  progress(work: string): Promise<StoredProgress | undefined>
  notes(work: string): Promise<FetchedNotes>
}

export interface Opened {
  session: Session
  etag: string | undefined
  /** An empty document where the notes read failed, never `undefined`. */
  notes: NotesDoc
  /** Absent where the read failed, which is what makes a later write refuse. */
  notesEtag: string | undefined
  /** The notes read's failure, for the caller to log. Absent on success. */
  notesError?: unknown
}

/**
 * Throws for "no works available" and for a work that defines no editions.
 * The notes read is the one failure that does not stop the reader: a reader
 * who cannot see their notes should still get their pages.
 */
export async function openFirstWork(ports: StartupPorts): Promise<Opened>

/** Every slot of one edition, in order. `Resolver` has no bulk accessor. */
export function allSlots(resolver: Resolver, edition: string): Position[]
```

**Stays behind:** the `useEffect`, its `cancelled` flag, and six setter calls.

---

### 4.8 `web/src/correction.ts` — the write, and the view of it *(extended)*

**Moves in:** 940–976 (the `c` case's target rule), 1768–1781
(`pickCounterpart`'s slot→`Position`), 1783–1884 (`confirmCorrection`),
2162–2193 (the four render derivations).

```ts
import type { CorrectionRequest, FetchedSidecar } from './api'
import type { Group } from './pairing'
import type { ReadingState } from './reading'
import type { Alignment } from './session'
import type { Session } from './startup'
import type { Position, Resolver } from './resolver'

export interface CorrectionPorts {
  patch(
    work: string,
    etag: string,
    correction: CorrectionRequest,
  ): Promise<FetchedSidecar>
  sidecar(work: string): Promise<FetchedSidecar>
  resolve(json: string): Promise<Resolver>
}

export type CorrectionOutcome =
  | {
      kind: 'landed'
      resolver: Resolver
      alignments: Alignment[]
      etag: string | undefined
      /** Where to land the reader, if `b` resolves to a slot. */
      slot: number | undefined
      to: string
    }
  | {
      kind: 'stale'
      /** Absent where the re-read itself failed; `pending` survives either way. */
      resolver?: Resolver
      alignments?: Alignment[]
      etag?: string
      notice: string
      /** The re-read's own failure, for the caller to log. */
      error?: unknown
    }
  | { kind: 'refused'; notice: string }

/**
 * Write one correction. **Never disposes anything** — the caller frees the
 * outgoing resolver only once a replacement exists, which is the ordering
 * `resolver.ts`'s `dispose` doc requires and a parse failure here must not
 * break.
 */
export async function submitCorrection(
  work: string,
  etag: string,
  pending: Pending & { b: Position },
  ports: CorrectionPorts,
): Promise<CorrectionOutcome>

/**
 * Install a landed correction. `previous` moves to the edition being left, as
 * it must on *every* change to `viewing` — see `session.ts`'s own doc on
 * `previous` for the drift skipping it produces.
 */
export function applyLanded(
  session: Session,
  landed: Extract<CorrectionOutcome, { kind: 'landed' }>,
): Session

/**
 * Which edition to correct against: the authority where the reader has swapped
 * away from it, otherwise `previous`. `undefined` where there is nothing to
 * correct against.
 */
export function counterpartEdition(
  state: ReadingState,
  previous: string,
  shownEdition: string,
): string | undefined

export interface CorrectionView {
  slots: Position[]
  /** `correctionTiles`, not `groups` — one tile per slot, so a spread's half
   *  can be named on its own. */
  tiles: Group[]
  pairedIndex: number | undefined
  displaced: Position | undefined
}

export function correctionView(
  pending: Pending,
  resolver: Resolver,
): CorrectionView
```

**Stays behind:** the four `useState`, `submittingRef`/`pendingRef`, the
`try`/`finally` around `setSubmitting`, and the one `resolver.dispose?.()` call.

---

### 4.9 `web/src/CorrectionBanner.tsx` — the confirm *(new component)*

80 lines of prose, two buttons and their `disabled`, and a consequence sentence
whose two branches say opposite things. It has no state and six props. It is the
easiest thing in this file to test and currently one of the least tested.

**Moves in:** 3147–3226.

```tsx
export interface CorrectionBannerProps {
  pending: Pending
  displaced: Position | undefined
  notice: string | undefined
  submitting: boolean
  onConfirm: () => void
  onCancel: () => void
}

export function CorrectionBanner(props: CorrectionBannerProps): ReactElement
```

---

### 4.10 `web/src/module.ts` — the preferences map *(extended)*

Every write to `prefsMap` is the same three shapes: replace one module's slice,
change one field of one slice, reset one slice. Six sites do it by hand today.

**Moves in:** 475–535 (the map's construction, the persist loop, `setMode`),
989–1027 and 1072–1081 (the three pref-writing key cases), 1980–2016 (both
settings callbacks).

```ts
/** One module's slice replaced. Spread, never assigned — see `setOverride`. */
export function withSlice<P extends object>(
  map: Record<string, object>,
  mod: ReaderModule<P>,
  next: P,
): Record<string, object>

/** One module's slice changed, given its current value. */
export function changeSlice<P extends object>(
  map: Record<string, object>,
  mod: ReaderModule<P>,
  change: (prefs: P) => P,
): Record<string, object>

/** Every registered module's slice, loaded. */
export function initialPrefs(
  modules: ReaderModule<unknown>[],
  load: (id: string, defaults: object) => object,
): Record<string, object>

/** Every registered module's slice, saved. Whole map, always — see App.tsx:491. */
export function persistPrefs(
  modules: ReaderModule<unknown>[],
  map: Record<string, object>,
  save: (id: string, prefs: object) => void,
): void

/** The settings panel's change, generic over every registered module. */
export function fieldChanged(
  map: Record<string, object>,
  registry: Registry,
  id: string,
  key: string,
  raw: unknown,
): Record<string, object>

/** The settings panel's reset, in memory. The caller still calls `resetPrefs`. */
export function sliceReset(
  map: Record<string, object>,
  registry: Registry,
  id: string,
): Record<string, object>
```

`fieldChanged` needs `settingsPanel.ts`'s `withFieldValue`; check for an import
cycle before writing it — `settingsPanel.ts` imports nothing from `module.ts`
today, so there should be none, but confirm rather than assume.

Alongside it, one line each in two module files, so the key cases become
one-liners like `direction`'s and `regroup`'s already are:

```ts
// modules/annotations.ts
export function toggleReveal(prefs: AnnotationsPrefs): AnnotationsPrefs

// modules/direction.ts — the `d` key's flip, which today is written out in App
export function flipped(sidecarReading: Direction, prefs: DirectionPrefs, edition: string): DirectionPrefs
```

---

### 4.11 `web/src/keyplan.ts` — the guard, and the route *(new)*

The keydown handler is 413 lines and 209 code lines — the largest single block
in the file and, by the estimate in §6, about a fifth of its statements. Almost
all of it is guards: thirteen early returns, each with a recorded reason, several
of which exist because of a measured defect (a refused save dropping focus to
`<body>`; `disabled={submitting}` blurring the just-clicked Confirm).

**This module is not the whole handler.** By the time it is written, §5's
sequencing has already moved every *action* into the module that owns it, so
what is left is the decision: given a key, an intent, and six guard flags, what
should happen and should the default be prevented.

**Moves in:** 850–1255 minus `recordSwap` (→ §4.3) and minus every case body
(→ §§4.1, 4.2, 4.8, 4.10).

```ts
import type { Intent } from './keys'
import type { ReadingIntent } from './session'
import type { PanelMode } from './notes'

/**
 * What the handler must know that the event does not carry.
 *
 * **Nine fields, not six.** The first draft had the six above the line below,
 * and left three more guards inline in the module `switch` — where they would
 * have made the residual switch three guarded cases rather than one-line calls,
 * and made §6's 45-line keydown estimate wrong. The three are here instead,
 * and each is a *decision about whether a key does anything*, which is the one
 * thing this module is for.
 *
 * The alternative was to leave them in the switch and raise the residue
 * estimate to about 55. It lost on the same argument §6 uses against a hook
 * holding the 412 retry: a guard left in `App.tsx` is a branch nothing can
 * reach, and three of them would be three uncovered branches in the file the
 * whole map exists to empty. What growing `Guards` costs is three lines
 * computed at the subscription, all three reading refs that are read there
 * already.
 */
export interface Guards {
  helpOpen: boolean
  panel: PanelMode | undefined
  /** A correction is being asserted — `pending` is set. */
  correcting: boolean
  /** A correction is on the wire. */
  submitting: boolean
  /** A note write is on the wire. */
  notesBusy: boolean
  /** `isInteractiveTarget(event.target)` — focus is in a control. */
  onControl: boolean
  // ---- the three the first draft missed -------------------------------------
  /**
   * `modules/zoom.ts`'s `zoomable(zoomPrefsRef.current, drawnRef.current !==
   * undefined)` — App.tsx:1126, 1139, 1146. One flag for the three zoom keys,
   * shared with §4.1's `pressTakesPan`; §4.2 has the argument for one
   * predicate rather than two fields.
   */
  zoomable: boolean
  /** `creditsPrefsRef.current.panel` — App.tsx:1113, what makes `C` inert. */
  creditsPanel: boolean
  /** `helpPrefsRef.current.enabled` — App.tsx:1156, what makes `?`/`h` inert. */
  helpEnabled: boolean
}

export type KeyAction =
  | { kind: 'close-help' }
  /** Flush and close. `Escape` means "I am finished here", not "abandon". */
  | { kind: 'close-editor' }
  | { kind: 'cancel-correction' }
  | { kind: 'toggle-chrome' }
  | { kind: 'begin-correction' }
  | { kind: 'module'; intent: string }
  /** `ReadingIntent` is `session.ts`'s own `Exclude` — reused rather than
   *  restated, so a fourth `Intent` kind cannot be admitted here silently. */
  | { kind: 'reading'; intent: ReadingIntent }

export interface Planned {
  action: KeyAction
  /**
   * `Tab` alone is `false`: toggling the chrome is layered on top of the
   * browser's own focus movement, and preventing it made every focusable
   * control in the reader mouse-only.
   */
  preventDefault: boolean
}

/**
 * `key` is checked before `intent`, because `Escape` is not one of `intentOf`'s
 * intents and deliberately does *not* stand down for a focused control — the
 * tile a reader just clicked is itself a button.
 *
 * `undefined` means the keypress belongs to something else: to a control that
 * has focus, to the note editor (nothing reaches the page while it is open), or
 * to a write that is still on the wire.
 */
export function keyPlan(
  key: string,
  intent: Intent | undefined,
  guards: Guards,
): Planned | undefined
```

**Stays behind:** the `window` subscription, the six mirror refs the plan reads
through, the three derived refs the three new guards are computed from
(`zoomPrefsRef`/`drawnRef`, `creditsPrefsRef`, `helpPrefsRef`), and one
`switch (action.kind)` of one-line calls.

**Two guards that genuinely cannot come here**, and they are why §6's keydown
residue is 45 lines rather than 30: `const current = sessionRef.current; if
(!current) return` and `if (shown.kind !== 'at') return` appear in the `c`, `n`,
`d` and `b` branches (955–958, 1029–1034, 992–993, 1013–1017). They dereference
the session ref and run `display()` against a live resolver, so they belong to
the residual dispatch and not to a plan that takes flags. Naming them here so
the estimate is not read as slack.

---

### 4.12 `web/src/modules/readouts.ts` and `modules/fit.ts` — two small rules *(extended)*

**This section is assigned to a task**, which the first draft's §5 forgot to do:
`hugForCell` lands in **Task 12** because `GroupViewProps.hug` is built from it
and §4.6 cannot be written without it; `readoutLines` lands in **Task 13**, with
the rest of the render derivations.

**Moves in:** 2275–2321 and 2356–2379 (`hugFor`).

**The readouts take the whole block, not two lines of it.** The first draft took
`reportFrom` and `reportTotal` — 2275–2296 — and left seven bindings behind, then
used that as evidence for §6's claim that "there are no more rules". 2275–2321 is
47 lines and **nine** bindings: `reportFrom`, `reportTotal`, `chapter`,
`positionText`, `chapterText`, `healthText`, `slotsLeft`, `timeText`,
`notesText`. Seven of the nine are already one-line calls into
`modules/readouts.ts` (`positionLine`, `chapterReadout`, `healthReadout`,
`gapHealth`, `timeLine`, `noteCountLine`) — but *which* of them to call, and with
what, is four branches and two arithmetic rules, and it is the arithmetic that is
worth the module: `?? slot + 1` for an edition the resolver cannot count,
`Math.max(total - (slot + 1), 0)` for a slot count that has gone backwards, and
the choice of `gapHealth` over `healthReadout` in a gap so that "the view and the
row beneath it cannot contradict each other".

```ts
// modules/readouts.ts
/**
 * Where the readouts report from. In a gap that is the authority slot the gap
 * was found at, with `name` set so the change of scale is never silent.
 */
export function reportPoint(
  shown: Display,
): { edition: string; slot: number; name: string | undefined }

/**
 * All four readout lines, from one `Display`. `Chrome` takes strings, and
 * every branch behind them is here rather than in the render body.
 *
 * `now` is a parameter, not `Date.now()` — `timeLine` is the one readout whose
 * output moves on its own, and a test that cannot fix the clock cannot assert
 * on it.
 */
export function readoutLines(
  shown: Display,
  resolver: Resolver,
  alignments: Alignment[],
  hub: string,
  notes: { here: number; total: number },
  clock: { startedAt: number; turns: number[]; now: number },
): {
  position: string
  chapter: string | undefined
  health: string | undefined
  time: string
  notes: string | undefined
}

// modules/fit.ts
/** Which edge a cell's artwork is pulled to. The *first* cell hugs its right. */
export function hugForCell(cell: Cell): ArtworkHug
```

`hugFor`'s mode/group guard stays in App — it needs `current`, which is bound
below the guard.

---

## 5. Step 5 — the sequencing

Four extractions, covering **§§4.1–4.12** — the first draft's sequencing covered
§§4.1–4.11 and left §4.12 assigned to nothing, which mattered because
`hugForCell` feeds `GroupViewProps.hug` and Task 12 cannot be written without it.
§4.12 now names its two tasks and they appear below.

**The plan allots three (Tasks 11–13); this map's fourth is a different kind of
task and §6 is why it exists.** Tasks 11–13 are module extractions and match the
plan's shape exactly. The fourth moves no logic at all.

### The constraints that force the order

1. **`keyplan.ts` must be last.** Its `KeyAction` union names operations that
   have to exist in the modules that own them. Extracting it first means
   inventing a union with no implementations behind it, and then rewriting it
   three times.
2. **Each cluster takes its own key cases with it.** This is what makes the key
   plan tractable: the module `switch` is 186 lines and 121 code lines today, and
   by the time §4.11 is written every case body has already left. The
   alternative — extract the whole handler first, then hollow out the cases — was
   rejected because it makes the largest and riskiest move the one with no
   modules under it to move *into*.
3. **`correction.ts` needs `Session` exported from `startup.ts`.** Same task, and
   `startup.ts` lands first inside it.
4. **`GroupView.tsx` and `secondCellAlone` land in one commit.** The `alone`
   decision and the JSX consuming it are the flicker fix; splitting them leaves a
   commit where the rule is in one file, the drawing in another, and no test over
   either.
5. **Rule 1 (no value import of `resolver.ts`) applies at every step,** including
   in the new test files. `fixtures.ts`'s `fakeResolver` is the seam.
6. **654 node tests green before and after every commit** — and, from Task 12,
   the jsdom project alongside them.

   *The first draft added a seventh constraint here: that Task 12 must be run
   locally against a downloaded Chromium, because the six browser tests were the
   only detector for §4.6's reconciliation invariant. **That constraint is
   deleted, because it was false.*** `PageView.browser.test.tsx` imports neither
   `App` nor `renderGroup`; all six of its tests mount hand-written JSX replicas
   of bare `PageView`s, and every one of them passes identically whether
   `renderGroup` exists, is extracted, or is rewritten — so they detect nothing
   about the extraction and the invariant currently has no detector at all. §4.6
   specifies one that runs in `jsdom`, which means it runs in CI, which is the
   coverage design's own §5 argument applied to a correctness test. The browser
   six should still pass, as they should after any change; there is no longer a
   scheduling constraint attached to them.

### The order, least coupled first

**Task 11 — the note editor, and the 412.** `notes.ts` extended (the `Draft`
reducer and the region rules), `history.ts` extended (`stepHistory`),
`notesWrite.ts` new (`saveNotes`), plus `PanelMode` moving out of
`NotePanel.tsx`. Least coupled: it reads `shown.edition` and `at.slot` as plain
arguments, writes nothing outside its own five states, and touches neither `view`
nor `session`. Takes the `n`, `a`, `z` and `Z` cases and the `Escape` note-panel
branch — **four** of `modules/annotations.ts`'s five bindings. The fifth, `r`
(1072–1081), is a preferences write and goes with §4.10 in Task 13; the first
draft gave it to both tasks, which made it the one range in the map claimed
twice.

Roughly 560 lines out of `App.tsx`, unchanged from the first draft — what
changed is that they land in three places rather than one, two of which already
exist and already have tests to extend. The largest genuinely new file in this
task is about a hundred lines.

**§4.1a is part of this task and is not optional.** The four-step sequence —
faithful port, correct tests, watch two of them fail, fix — is what makes the
one behaviour change in Task 11 legible in the diff. A Task 11 report that does
not quote the failing output has not done step 3.

**Task 12 — the view.** `modules/zoom.ts` extended (including `zoomable`),
`swapView.ts`, `modules/prefetch.ts` extended, `pairing.ts`'s `secondCellAlone`,
`modules/fit.ts`'s `hugForCell` (§4.12), `GroupView.tsx`, and the three jsdom
tests §4.6 specifies. Second because it is independent of Task 11 and of the
session, but it must precede the key plan, which routes the three zoom keys and
calls `carrySwap`. **`hugForCell` lands before `GroupView.tsx` inside the task**,
for the reason constraint 3 gives about `Session`: `GroupViewProps.hug` is built
from it.

This is also the task that stands up the `jsdom` project, so its first commit is
config rather than code. Roughly 480 lines out.

**Task 13 — the session, the correction, the preferences, the readouts, the key
plan.** `startup.ts`, then `correction.ts` extended, `CorrectionBanner.tsx`,
`module.ts` extended (with `modules/annotations.ts`'s `toggleReveal` and the `r`
case, and `modules/direction.ts`), `modules/readouts.ts` extended with
`readoutLines` (§4.12), then `keyplan.ts`. Last because every constraint above
points here. Roughly 740 lines out — the largest of the three, and unavoidably
so, because the key plan cannot be earlier and the rest of this task is what
makes it small.

**Task 13a — the residue pass.** `PageArea.tsx`, `hooks.ts`. Not a module
extraction: it moves no branching logic, only the render subtree and the
plumbing effects that hold no decision. It exists because §6 measured that
Tasks 11–13 do not on their own bring `App()` under the brief's bound, and it
is separate from Task 13 because folding it in would make one task both the
largest *and* the one that must land last — a failure there would block Task
14 entirely.

---

## 6. Step 6 — the residue, and the one place this map misses its bound

### The estimate, block by block

Every "stays" below is a judgement made by walking the block and marking what
moves; the totals are arithmetic over those judgements. **This is an estimate,
not a measurement** — the measurement is Task 13a's `wc -l`.

| block | lines | after 11–13 | after 13a | measured, post-13 |
| ----- | ----- | ----------- | --------- | --- |
| declarations 335–777 | 443 | 150 | 60 | **415** (116 code / 274 comment / 25 blank) |
| load effect 779–841 | 63 | 12 | 12 | **22** |
| keydown 843–1255 | 413 | 45 | 45 | **278** (174 code / 104 comment) |
| debounce refs 1257–1304 | 48 | 20 | 20 | not re-measured |
| flush / progress / turns / tick / scroll effects | 150 | 117 | 15 | not re-measured |
| prefetch effect 1461–1547 | 87 | 30 | 8 | not re-measured |
| wheel effect 1549–1630 | 82 | 35 | 8 | not re-measured |
| swap effect 1632–1730 | 99 | 20 | 8 | not re-measured |
| guard + navigation 1732–1781 | 49 | 31 | 31 | **44** (post-Finding-3; 50 before it) |
| `confirmCorrection` 1783–1884 | 102 | 8 | 8 | **54** |
| notes writes 1886–1978 | 93 | 10 | 10 | not re-measured |
| settings callbacks 1980–2040 | 60 | 11 | 11 | not re-measured |
| render derivations 2042–2354 | 313 | **75** | **75** | not re-measured |
| `hugFor` / `drawnRef` 2356–2437 | 82 | 25 | 25 | not re-measured |
| region gesture 2439–2715 | 275 | 60 | 60 | not re-measured |
| `renderGroup` 2717–2889 | 173 | 0 | 0 | 0, confirmed |
| gap moves 2891–2914 | 24 | 20 | 20 | not re-measured |
| return JSX 2916–3358 | 443 | 363 | 196 | 373, confirmed close |
| **total** | **3,026** | **≈ 1,050** | **≈ 615** | **2,361** (`App()` measured directly, not a sum of this column) |

### Five rows are now known-wrong, measured rather than estimated

**The keydown row was defended rather than corrected, and a review of this
document caught it.** Task 13's own report read ≈45 as `onKeyDown`'s whole
block and argued the 275 lines actually found there were mostly residue
already assigned elsewhere by name. That defence does not hold: §4.11 says the
two session guards inside the `c`/`n`/`d`/`b` branches are "why §6's keydown
residue is 45 lines rather than 30", which only makes sense if those branches
were already inside the 45; §4.3 names `recordSwap` (1221–1252) as moving to
`swapView.ts` and lists only `swapRef`, `pageAreaRef`, the `load` subscription
and its cleanup as what stays, leaving its 31-line residual wrapper unassigned;
and §3 assigns the seventh `setView` site — `setView(clampRect)` at the turn —
to this block by name, not away from it. Only `onKeyDown`'s own 50-line
dispatch was ever actually priced into the 45.

Measured directly over the effect — `useEffect(() => {` at its subscription to
the closing `}, [session?.resolver, session?.editions])`, the justification
comment above it excluded, the same rule applied to every row below — the
block is **278 lines: 174 code, 104 comment**, against the table's ≈45. (This
task's own Finding 2 fix adds four lines — three local `zoom` captures — inside
this same effect; the code as reviewed measured 274/170/104. Both numbers are
real; 278/174/104 is what the committed tree now holds.)

**Four more rows are known-wrong for the same reason: nobody subtracted the
residue §4 and §6 already assign to them by name, and this task re-measured
them while it was in the file for the keydown fix.**

- **declarations 335–777**, estimated to fall to 150: **measured 415** (116
  code, 274 comment, 25 blank). Most of the gap is refs that did not move
  because their *logic* moved and left the ref behind as the bridge to it —
  the nine mirrors `keyplan.ts` reads through, the derived refs its three new
  guards are computed from, `swapRef`/`pageAreaRef` for `swapView.ts`, and
  `drawnRef` for `hugForCell` and `frameOf` alike. Tasks 11–13 moved logic out;
  none of it could take the ref that hands that logic its state.
- **load effect 779–841**, estimated 12: **measured 22.** `startup.ts` took
  the five reads; the `cancelled` flag, the effect wrapper and five setters
  were never claimed by §4.7 and did not move.
- **`confirmCorrection` 1783–1884**, estimated 8: **measured 54** (37 code, 17
  comment). §4.8's own "stays behind" list names the four `useState`,
  `submittingRef`/`pendingRef`, the `try`/`finally` and the one `dispose?.()`
  call — that is most of a 54-line function, not an 8-line one, and the
  estimate never summed them.
- **guard + navigation 1732–1781**, estimated 31: **measured 44** (50 before
  this task's Finding 3 fix). `pickCounterpart`'s slot→`Position` conversion is
  now `correction.ts`'s `counterpartPick`, per §4.8's own moves-in list and
  this task's third finding — Task 13's report had recorded the move as
  impossible, which was itself wrong. What the 31 estimate never priced in is
  `take`, `goToSlot` and the `if (!session) return null` guard above them, none
  of which any section assigns anywhere.

**`renderGroup` (0) and the return JSX (measured 373 against 363) were
spot-checked and are not known-wrong** — `renderGroup` is confirmed fully
gone, and the JSX gap is inside the estimate's own noise. **The remaining
rows were not independently re-measured in this pass** and should be read as
unverified estimate, not as confirmed by this correction.

**The total is worse than these five rows alone suggest.** `App()` measured
directly — `function App() {` to its closing brace, independent of any row
breakdown — is **2,361 lines**, against the table's **≈1,050** "after 11–13"
estimate: more than double it. **≈615 for "after 13a" should be treated as
stale.** Task 13a's own described scope below is `PageArea.tsx` (≈167 lines)
plus `hooks.ts` (≈250 lines) — about 417 lines out, which from a measured
2,361 leaves roughly 1,944, not 615. Whoever picks up Task 13a should plan
against 2,361 and this section's corrected rows, not against ≈615, and should
expect the true post-13a residue to land well above the brief's bound rather
than comfortably under it.

**Two rows moved in this revision** (below), during the estimate's own
drafting, before any of Tasks 11–13 ran — a different thing from the five
rows just corrected, which are what the finished code now measures against
that estimate. **They very nearly cancel.**

- **Declarations, 150 → 168.** §4.6 keeps `drawnSrcs` and its two setters
  (591–608) in `App` rather than moving them into `GroupView`, because
  component-local state does not survive the unmount a two → scroll → two
  excursion causes. Eighteen lines back. In the `after 13a` column it is still
  60: `hooks.ts` takes it as `useDrawnSrcs`, one `useState` and two setters with
  no branch, which is exactly what Rule 2 lets that file hold.
- **Render derivations, 92 → 75.** §4.12 now takes the whole readouts block
  (2275–2321) rather than its first two bindings, which is 25 more lines of
  derivation and comment out and about 8 lines of one `readoutLines(...)` call
  back in.

Net **+1 line** on a figure whose own precision is ±50, so **the `after 11–13`
estimate does not move: still ≈ 1,050.** The `after 13a` figure drops from ≈ 630
to ≈ 615. Both are still estimates, and Task 13a's `wc -l` is still the
measurement.

Of the ≈ 615, roughly 275 are code and 340 are comment — close to the 50/50 split
the whole function has, because the comments that stay are the ones about the
wiring.

*Both ≈ 1,050 and ≈ 615 are the pre-implementation estimate, unchanged since
before Tasks 11–13 ran. "Five rows are now known-wrong", above, is what the
finished code actually measures against it — 2,361, not ≈ 1,050 — and is the
figure to read the rest of this section against.*

### The gate, answered honestly

**Tasks 11–13 leave `App()` at about 1,050 lines. That is over the brief's
bound of about 500, and the map does not stop there.** Two questions had to be
answered before adding Task 13a, and both were:

**Is the grouping too coarse?** No — checked, and the check is what produced
Task 13a. At 1,050 lines the residue is 363 lines of render tree, 168 of hook
declarations, 117 of subscription effects and about 200 of dependency-array
reasoning. **None of it is logic that a finer module boundary could take.** A
finer grouping would split rules that the inventory shows move together — and
§3's rejection of "correction and history" is what that mistake looks like from
the other side. The residue is not coarse grouping; it is React.

**Would more pure modules help?** Two more, and this revision took both — but
not enough to change the answer.

*The first draft wrote "there are no more rules", and that was an overstatement
the review caught.* There were two, and both are now in §4.12: the readouts block
at 2275–2321, which is 47 lines and nine bindings holding four branches and two
arithmetic rules (`?? slot + 1`, `Math.max(…, 0)`) plus the `gapHealth`-in-a-gap
choice; and `hugForCell`, which the first draft had already found. And there are
two smaller ones it still declines — see decisions 9 and 10 below.

What survives the correction is the shape of the answer rather than the word
"no". §0 measured `App()` at 50.8% comment. The remaining comments are
`biome-ignore` arguments about dependency arrays, the standing reason six mirror
refs exist, and the note panel's two-guard proof — every one of them about wiring
that stays. Taking the readouts whole moved the estimate by seventeen lines out
of a thousand, which is what "the residue is React, not coarse grouping" looks
like when it is tested rather than asserted.

**Is Task 13a still needed? Yes, and by a wider margin than the first draft
implied.** Tasks 11–13 leave ≈ 1,050 lines against a bound of about 500 — more
than double it — so the fourth task is not a rounding matter. What the first
draft left unsaid, and this revision says plainly: **≈ 615 is also over 500.**
The brief's bound is a line count and this function is half documentation, so
the honest reading of it is the ~275 code lines inside the ≈ 615, which is
comfortably under. *That is a judgement about what the bound was measuring, not a
measurement* — and it is the judgement §0 was written to support. If a reader of
Task 13a's `wc -l` disagrees, the argument to have is about the bound, not about
adding a fifth task: §4 has no module left to name.

So Task 13a takes the two things that are left and are not rules:

- **`PageArea.tsx`** — App.tsx 2949–3115, the page area with its pointer
  handlers, the two `PageAnnotations` mount sites, `NoteList` in `side`
  placement, and the gap/scroll/`Viewport` branch. 167 lines, ~20 props, one
  more file in the `jsdom` project. It is the O25 fix's one mount site and
  deserves a test of its own more than it deserves to be inline.
- **`hooks.ts`** — the nine one-line mirrors and the five subscription effects
  (`useLatest`, `useDecodeQueues`, `useWindowWidth`, `useFlushOnHide`,
  `useProgressWrite`, `useReadingClock`, `useScrollToCurrent`). About 250 lines
  moved, each hook holding at most one branch.

**`hooks.ts` is the one place Rule 2 is spent, and it is spent deliberately.**
It is allowed only because these effects hold no branch worth a unit test: the
decisions they used to contain have already gone to pure modules by then, and
what is left is `addEventListener`/`removeEventListener` and a ref assignment.
A hook holding the 412 retry would be the version of this that fails — it would
move statements out of `App.tsx`'s 70% bar and into the global threshold while
covering nothing, which is precisely the flattering-number failure this whole
branch exists to prevent.

### What `App()` is actually left holding

Twenty-odd hook calls, one `if (!session) return null`, four panel booleans,
about 75 lines of render derivations that are one-line calls into tested
modules, six callbacks handed to child components, and the render tree. Every
decision left in it, in full:

1. `shown.kind === 'at'` → `at` (one ternary).
2. `currentIndex` — `findIndex`, and the deliberate absence of a `Math.max(…, 0)`
   clamp.
3. `buffered` — `mode !== 'scroll' && prefetchPrefs.enabled`.
4. `overlayPrefs` — force `reveal` while the editor is open.
5. Which of `slots`/`grouped` the filmstrip gets while correcting.
6. `altAt`'s one-based page, and its `half` branch.
7. `hugFor`'s `mode !== 'two' || !group` guard.
8. The two `PageAnnotations` `pending` conditions.
9. `toMove`'s `page + 1` (2909–2910) — the display conversion `altAt` makes at
   the same kind of boundary, applied to `Gap`'s two offers.
10. `stripAnchor` (2911–2914) — `before` before `after`, because "a reader who
    has just walked forward into a gap came from behind it".

Ten decisions, none longer than two lines, every one of them reached by mounting
the component once. *The first draft listed eight and left 9 and 10 out; they are
in the gap-moves block, which its own table shows staying, so they were residue
the "every decision, in full" list did not name.*

**9 and 10 could go to `nearest.ts`**, which already owns `nearestPaired` and
`NearestOffer` and is where a reader would look for a rule about which offer
comes first. They are not moved, and the alternative that beat it is inertia with
a number attached: four lines of code and four of comment, against a module
boundary crossed to reach them and one more import in the file this map is trying
to empty. Recorded rather than done, so the next person to touch `Gap` can decide
with the argument in front of them.

**Task 14's ≥70% bar on `App.tsx` should come in comfortably, and if it comes in
far above 70% that is the signal the plan already names — that the extraction
left more behind than intended.**

### The statement estimate

`App.tsx` holds 737 statements over 1,615 code lines including the top level —
0.456 statements per code line, measured. Residual code after Task 13a is about
275 lines in `App()` plus about 40 at the top level, so **roughly 145 statements
remain, down from 737 — an 80% reduction.** Guess, not measurement: statements
are denser in logic than in JSX, and what stays is disproportionately JSX, so
the true figure is more likely under 145 than over it. *(The first draft said
270 lines and 140 statements; this revision's two offsetting corrections to the
line estimate move the code figure by about five lines, which is inside the
guess's own error bar and is restated only so the two numbers agree.)*

**Overtaken by measurement, same as the line estimate above.**
`web/coverage/coverage-summary.json` puts `App.tsx` at **578 statements**,
measured after Tasks 11–13 and unchanged by this task's own two code fixes —
before Task 13a has touched anything, and already four times ≈145. The
0.456-per-code-line ratio the guess was built on does not hold against the
"known-wrong" rows above: `confirmCorrection`'s 37 code lines carry a
`try`/`finally`, three outcome branches and a re-entry guard, not JSX.

---

## 7. For the implementer

- **Every module here is `.ts` and imports no React**, except `GroupView.tsx`,
  `CorrectionBanner.tsx`, `PageArea.tsx` and `hooks.ts`. If a `.ts` file in this
  map needs a React import, the boundary is in the wrong place — stop and say so
  rather than adding one.
- **Rule 1 is a build failure, not a style point.** `import type { Resolver }`,
  never `import { wasmResolver }`. A test that gets this wrong passes on your
  machine.
- **`sameNotes` compares by `JSON.stringify`, deliberately** (App.tsx:195–201) —
  reference equality would record a step for a write that changed nothing and
  leave undo needing two presses. Do not "fix" it on the way past.
- **`writeNotesLatestRef` is not an accident.** `debounce` holds the callback it
  was built with for the component's life, and `writeNotes` closes over state
  that changes every render. Whatever replaces it has to keep that property.
  **And it is not the only render-closure read in this area** — the one at
  App.tsx:1939/1942 is the defect §4.1a exists for. When you replace one, look
  at the other.
- **The `restored` and `restoredTurn` skips are one run each, not a general
  guard.** Both have measurements behind them — the second produced a `~6m left`
  estimate derived entirely from load latency before a key had been pressed.
- **§4.1a is the first thing Task 11 does, and step 3 is the deliverable.**
  Write the faithful port, write the ten tests, run them, and quote the two that
  fail. A green suite at the end of Task 11 with no record of them failing first
  proves the tests exist, not that they detect anything.
- **Two things must be watched failing before they are made to pass:** §4.1a's
  cases 2, 3 and 4, against the faithful port; and §4.6's node-identity test,
  against the pre-extraction `renderGroup` shape. Both are cheap to arrange and
  both are the only evidence the tests are detectors rather than descriptions.
- **The six browser tests carry no scheduling constraint.** The first draft said
  Task 12 had to be run locally against a downloaded Chromium because those six
  were the only detector for §4.6's invariant. They import neither `App` nor
  `renderGroup` and detect nothing about this work. Run them if convenient;
  §4.6's jsdom tests are what actually guards the extraction, and they run in CI.
- **`drawnSrcs` stays in `App`.** If `GroupView` ends up holding it, the two →
  scroll → two path reintroduces the flicker that state was added to fix, and
  §4.6's third jsdom test is what says so.
- **Record the `notesBusy` difference in §4.1** in the Task 11 report — along
  with the 412 fix, which is the larger of the two and the one a reader's data
  depends on. Under React 19's automatic batching the `notesBusy` difference is
  very likely not observable at all; disclose it anyway, and if counting renders
  in the Task 11 tests can settle it cheaply, settle it.

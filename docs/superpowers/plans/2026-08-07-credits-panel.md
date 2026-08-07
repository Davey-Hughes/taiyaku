# The credit-pages panel, and a skip that governs something — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make both "Credit pages" settings toggles govern something — `C` opens
a panel listing the viewed edition's credit pages, and `skip` keeps them out of
the reading flow.

**Architecture:** `skip` filters *groups*, not slots, so one change governs the
page area, the arrow keys and the filmstrip together. Slot numbers are never
touched. Two dead functions go: `creditSlots` (a second implementation of slot
expansion) is replaced by a filter over the `Position[]` `allSlots` already
produces, and `nextReadable` (a slot walker, in a reader that steps by group) is
retired in favour of a fallback inside `stepToGroup`.

**Tech Stack:** React 19, TypeScript, Vite, `vitest`. No new dependencies.

Design: [`../specs/2026-08-07-credits-panel-design.md`](../specs/2026-08-07-credits-panel-design.md).

## Global Constraints

- **Every npm command runs from `web/`.** `npm test`, `npm run typecheck`,
  `npm run lint`. Repo root is `/home/davey/projects/taiyaku`.
- **`npm run lint` is `oxlint --deny-warnings`** — a warning fails it.
- **No new dependencies.** `package.json` must not change.
- **Zero-based internally, 1-based only at a boundary a person reads.**
  `Resolver.credits` returns zero-based *page* indices; slots are a different
  space from pages and the two diverge from the first joined spread onward.
  Confusing them is the defect this project has hit most often — `en-fan` page
  234 is slot 239. The only `+ 1` is immediately before display text.
- **Skipping never renumbers.** `docs/sidecar.md` is normative: the slot is the
  addressed unit. Nothing here may change `slots.length`, the position readout's
  total, or what any slot is called.
- **Comments explain why, not what** — naming the alternative rejected and the
  evidence. Match the density of the file you are editing; it is deliberately
  high.
- **Branch:** `credits-panel`, already created and holding the design doc. It is
  stacked on `o25-chrome-in-a-gap`, which is unmerged as PR #16.

**Tasks 1, 2 and 3 touch disjoint files and can run in parallel.** Task 4
consumes all three. Task 5 consumes Task 4.

---

### Task 1: `creditSlotsIn` replaces `creditSlots`, and `nextReadable` goes

`creditSlots`' own doc says it "must be replaced when the panel is mounted, not
adopted as it stands", for two reasons: it re-derives slot expansion that
`taiyaku_core::slot::expand` owns, and its `Pagination` argument cannot be built
from anything `web/` has. Both are answered by filtering the `Position[]` that
`App.tsx`'s `allSlots(resolver, edition)` already returns — which comes through
the `Resolver`, which is `taiyaku-wasm`'s `Binding`, which is core's own
expansion. One derivation, read rather than repeated.

`nextReadable` goes in the same task because it lives in the same file and has
the same status: exported, tested, called by nothing. See the design's §3 for
why it is not wired instead.

**Files:**
- Modify: `web/src/modules/credits.ts`
- Test: `web/src/modules/credits.test.ts`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `creditSlotsIn(slots: Position[], pages: number[]): number[]`. Task
  4 calls it with `allSlots(resolver, shown.edition)`.
- Removes: `creditSlots`, `Pagination`, `nextReadable`.

- [ ] **Step 1: Write the failing tests**

In `web/src/modules/credits.test.ts`, replace the whole `creditSlots` describe
block, and delete the `nextReadable` block and the `skipping is presentational`
block entirely — the latter moves to `pairing.test.ts` in Task 2, restated
against groups, because the claim it pins is now a property of the group filter.

Change the import line to:

```ts
import { creditSlotsIn, creditsModule, PANEL_INTENT } from './credits'
```

Replace the two fixtures (`spreadAt1`, `flat`) and the `creditSlots` block with:

```ts
import type { Position } from '../resolver'

/** Four pages, page 1 a spread — the slot table `allSlots` would return. */
const spreadAt1: Position[] = [
  { page: 0 },
  { page: 1, half: 'R' },
  { page: 1, half: 'L' },
  { page: 2 },
  { page: 3 },
]
/** Four pages, no spreads: slot index equals page index. */
const flat: Position[] = [{ page: 0 }, { page: 1 }, { page: 2 }, { page: 3 }]

describe('creditSlotsIn', () => {
  it('converts page indices to every slot that page presents', () => {
    // Page 1 is a spread, so skipping it must skip both halves — skipping one
    // would leave the reader on half a credits card. Both halves carry the
    // same `page`, which is why filtering on `page` gets this for free rather
    // than needing the spread list the old `Pagination` asked for.
    expect(creditSlotsIn(spreadAt1, [1])).toEqual([1, 2])
  })

  it('is the identity on an edition with no spreads', () => {
    expect(creditSlotsIn(flat, [0, 3])).toEqual([0, 3])
  })

  it('ignores a page the edition does not have', () => {
    // The sidecar and the slot table are separate documents; a `credits` entry
    // naming a page past the end must not produce a slot number that indexes
    // nothing. Returns what it can rather than throwing — the panel and the
    // skip both degrade to "fewer pages", which is the safe direction.
    expect(creditSlotsIn(flat, [2, 99])).toEqual([2])
  })

  it('answers nothing for an empty credits list', () => {
    // The common case today: no corpus sidecar has a `credits` field at all.
    expect(creditSlotsIn(flat, [])).toEqual([])
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

```sh
cd web && npm test -- credits
```

Expected: FAIL with `creditSlotsIn is not a function`.

- [ ] **Step 3: Write the implementation**

In `web/src/modules/credits.ts`, add the import at the top:

```ts
import type { Position } from '../resolver'
```

Delete `Pagination`, `creditSlots` and `nextReadable` entirely — declarations
and doc comments — and put this in `creditSlots`' place:

```ts
/**
 * Every slot of `slots` that one of `pages` presents.
 *
 * The sidecar's `credits` is in pages and the reader addresses slots, so this
 * is the one place the two meet. A joined spread presents two slots and both
 * are returned: half a credits card is not a page anybody wants, and both
 * halves carry the same `page`, so filtering on `page` gets that without
 * needing to know which pages are spreads.
 *
 * **`slots` is `App.tsx`'s `allSlots(resolver, edition)`, deliberately.** The
 * function this replaced took a `{ pages, spreads }` and walked it, which was
 * a second implementation of slot expansion — `taiyaku_core`'s `slot::expand`
 * owns that derivation and `docs/sidecar.md` is normative on the page
 * sequence; `docs/status.md` names two consumers deriving it separately as the
 * defect `taiyaku-read` exists to prevent, because the format cannot detect
 * the disagreement. Taking the already-expanded table instead means the
 * browser reads core's answer rather than recomputing it in a third language.
 * It also removes the reason the old signature was uncallable: nothing in
 * `web/` could build a `{ pages, spreads }`, since `Resolver` exposes neither
 * a page count nor a spread list.
 *
 * A page the edition does not have contributes nothing rather than throwing.
 * The sidecar's `credits` and the slot table are separate documents and can
 * disagree after a re-solve; degrading to "fewer skipped pages" is the safe
 * direction, where a thrown error would take the whole reader down over a
 * stale list.
 */
export function creditSlotsIn(slots: Position[], pages: number[]): number[] {
  const wanted = new Set(pages)
  const out: number[] = []
  slots.forEach((position, slot) => {
    if (wanted.has(position.page)) out.push(slot)
  })
  return out
}
```

Then update the module's own doc comment: the paragraph beginning "**O25 is
closed and this is no longer blocked, only unwritten.**" says `creditSlots` is
"the other thing to finish". Replace that sentence with a statement that it has
been replaced by `creditSlotsIn`, and that the panel is being mounted.

- [ ] **Step 4: Run the tests to verify they pass**

```sh
cd web && npm test -- credits && npm run typecheck && npm run lint
```

Expected: `credits` tests PASS, typecheck clean, lint clean. Typecheck is the
check that matters here — it proves nothing else imported the three removed
exports.

- [ ] **Step 5: Commit**

```sh
git add web/src/modules/credits.ts web/src/modules/credits.test.ts
git commit -m "credits: one slot expansion, read rather than repeated"
```

---

### Task 2: `readableGroups`, and a `stepToGroup` that can leave a skipped slot

**Files:**
- Modify: `web/src/pairing.ts`
- Test: `web/src/pairing.test.ts`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `readableGroups(all: Group[], skipped: Set<number>): Group[]`, and
  `stepToGroup`'s existing signature with new behaviour when the slot is in no
  group. Task 4 calls `readableGroups(groups(...), skippedSlots)`.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/pairing.test.ts`. `slotsWith` already exists inside the
`stepToGroup` describe block — these tests need it too, so **move it up to
module scope** (just below the imports) and leave the existing block using it
unchanged.

Add `readableGroups` to the import from `./pairing`.

```ts
describe('readableGroups', () => {
  it('drops a group whose every slot is skipped', () => {
    const g = groups(slotsWith(6, []), 'single', 'rtl')
    const left = readableGroups(g, new Set([3]))
    expect(left.map((x) => x.slots)).toEqual([[0], [1], [2], [4], [5]])
  })

  it('drops a joined spread whole when both halves are skipped', () => {
    // Slots 2 and 3 are page 2's two halves — one credits card, one tile.
    const g = groups(slotsWith(5, [2]), 'single', 'rtl')
    const left = readableGroups(g, new Set([2, 3]))
    expect(left.map((x) => x.slots)).toEqual([[0], [1], [4], [5]])
  })

  it('keeps a two-page group where only one slot is skipped', () => {
    // The rule that matters: `two` mode can pair a credit page with an
    // ordinary one, and dropping that group would hide a page the reader
    // wants. A credit page briefly on screen is the lesser cost — the reader
    // asked for the pairing, not for the deletion.
    const g = groups(slotsWith(6, []), 'two', 'rtl')
    const paired = g.find((x) => x.slots.length === 2)!
    const left = readableGroups(g, new Set([paired.slots[0]]))
    expect(left).toContainEqual(paired)
  })

  it('returns the very same array when nothing is skipped', () => {
    // Identity, not a copy: `skip` switched off must cost nothing, which is
    // module.ts's contract that a module which is off contributes nothing.
    const g = groups(slotsWith(6, []), 'single', 'rtl')
    expect(readableGroups(g, new Set())).toBe(g)
  })
})

describe('stepToGroup, standing on a slot no group holds', () => {
  // Reachable three ways once `skip` filters groups: taking a credit page
  // from the panel, clicking its filmstrip tile before switching `skip` on,
  // and switching `skip` on while already standing there. Before the credits
  // panel this could not happen, and the function simply refused to move.

  it('steps forward to the nearest surviving group', () => {
    const g = readableGroups(groups(slotsWith(6, []), 'single', 'rtl'), new Set([3]))
    expect(stepToGroup(g, 3, 1)).toBe(4)
  })

  it('steps backward to the nearest surviving group', () => {
    const g = readableGroups(groups(slotsWith(6, []), 'single', 'rtl'), new Set([3]))
    expect(stepToGroup(g, 3, -1)).toBe(2)
  })

  it('stays put when nothing survives ahead', () => {
    // The boundary answer the reader already gives at the first and last
    // slot: an arrow that has nowhere to go does nothing, rather than
    // jumping to the far end of the volume.
    const g = readableGroups(groups(slotsWith(6, []), 'single', 'rtl'), new Set([3, 4, 5]))
    expect(stepToGroup(g, 3, 1)).toBe(3)
  })

  it('crosses a run of skipped slots in one press', () => {
    const g = readableGroups(groups(slotsWith(6, []), 'single', 'rtl'), new Set([1, 2, 3]))
    expect(stepToGroup(g, 0, 1)).toBe(4)
  })
})

describe('skipping is presentational', () => {
  // Moved here from `modules/credits.test.ts` when `nextReadable` was retired:
  // the claim is now a property of the group filter, not of a slot walker.
  // The whole content of the numbering decision. If someone later renumbers
  // for readability, this goes red — which is the point.
  it('changes which slot an arrow lands on, never what a slot is called', () => {
    const slots = slotsWith(6, [])
    const all = groups(slots, 'single', 'rtl')

    const walk = (from: Group[]) => {
      const visited: number[] = []
      let at = 0
      for (let i = 0; i < 3; i++) {
        at = stepToGroup(from, at, 1)
        visited.push(at)
      }
      return visited
    }

    expect(walk(readableGroups(all, new Set([2, 3])))).toEqual([1, 4, 5])
    expect(walk(readableGroups(all, new Set()))).toEqual([1, 2, 3])

    // Same slot table in both worlds — only which entries are reachable
    // differs. `slots.length` is what the position readout divides by.
    expect(slots.length).toBe(6)
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

```sh
cd web && npm test -- pairing
```

Expected: FAIL — `readableGroups is not a function`, and the three stranded
cases return `3`, `3` and `0` from the current `stepToGroup`.

- [ ] **Step 3: Write `readableGroups`**

In `web/src/pairing.ts`, after `groups`:

```ts
/**
 * The groups a reader can actually reach, with skipped ones removed.
 *
 * **Filtering happens here rather than on `slots`, and rather than in the step
 * itself.** `App.tsx` hands one grouping to the page area, to `groupedRef` for
 * the arrow keys, and to `Chrome` for the filmstrip — so removing a group
 * takes a page out of all three at once, which is what "out of the reading
 * flow" means. Filtering `slots` instead would break the array-index-is-slot-
 * number correspondence every other consumer depends on, and filtering in the
 * step would leave the page on screen in the strip and in scroll mode.
 *
 * **Slot numbers survive untouched**, because `Group.slots` holds indices into
 * the unfiltered table. That is `docs/status.md` §4's "skipping never
 * renumbers": the position readout still divides by the whole edition, so it
 * jumps rather than counting differently. A per-reader toggle that changed
 * what an address meant would make one device's progress and another's notes
 * wrong on a third.
 *
 * **A group goes only when *every* slot in it is skipped.** `two` mode can
 * pair a credit page with an ordinary one, and dropping that group would hide
 * a page the reader wants — a worse failure than a credit page appearing
 * beside the page it was paired with. A joined spread is always both-or-
 * neither, since its two slots share a `page` and `creditSlotsIn` returns
 * both.
 *
 * Returns `all` itself when nothing is skipped, so a reader with `skip` off
 * pays nothing at all — `module.ts`'s contract that a module which is off
 * contributes nothing, and the property M5b's manual pass checks for.
 */
export function readableGroups(all: Group[], skipped: Set<number>): Group[] {
  if (skipped.size === 0) return all
  return all.filter((group) => !group.slots.every((slot) => skipped.has(slot)))
}
```

- [ ] **Step 4: Give `stepToGroup` its stranded case**

Replace the body of `stepToGroup`:

```ts
export function stepToGroup(all: Group[], slot: number, by: 1 | -1): number {
  const here = all.findIndex((group) => group.slots.includes(slot))
  if (here === -1) return nearestGroup(all, slot, by)
  const next = Math.min(Math.max(here + by, 0), all.length - 1)
  return all[next].slots[0]
}

/**
 * Where an arrow goes from a slot no group holds.
 *
 * Only `readableGroups` produces that state — a reader standing on a page
 * `skip` has removed, reached by taking it from the credits panel, by clicking
 * its filmstrip tile before switching `skip` on, or by switching `skip` on
 * where they already were. Before the credits panel it was unreachable, and
 * `stepToGroup` simply returned the slot: an arrow that did nothing, with no
 * way out except the panel that put you there.
 *
 * **This needs no knowledge of which slots are skipped**, which is the whole
 * argument for filtering in group space. `all` is already the filtered list,
 * so "the nearest group in the direction of travel" is the answer by
 * construction — no skipped set has to be threaded down here, and this stays a
 * pure function of the grouping.
 *
 * Returns `slot` unchanged when nothing survives ahead, which is the same
 * boundary answer the reader already gives at the first and last slot: an
 * arrow with nowhere to go does nothing, rather than jumping to the far end.
 */
function nearestGroup(all: Group[], slot: number, by: 1 | -1): number {
  if (by === 1) {
    const ahead = all.find((group) => group.slots[0] > slot)
    return ahead ? ahead.slots[0] : slot
  }
  // Groups are in slot order, so the last one ending below `slot` is the
  // nearest behind it.
  let behind: Group | undefined
  for (const group of all) {
    if (group.slots[group.slots.length - 1] < slot) behind = group
    else break
  }
  return behind ? behind.slots[0] : slot
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```sh
cd web && npm test -- pairing && npm run typecheck && npm run lint
```

Expected: all `pairing` tests PASS — including the ones that existed before,
which must not change — typecheck clean, lint clean.

- [ ] **Step 6: Commit**

```sh
git add web/src/pairing.ts web/src/pairing.test.ts
git commit -m "pairing: skip filters groups, and an arrow can leave a skipped page"
```

---

### Task 3: `CreditsPanel`

**Files:**
- Create: `web/src/CreditsPanel.tsx`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `CreditsPanel` and `CreditsPanelProps` as written below. Task 4
  mounts it.

**No unit test**, for the reason the O25 branch established: this repo has no
component tests and no DOM harness by design — every `*.test.ts` covers a pure
module. Do not add a framework. The gate is `typecheck` and `lint`; the
behaviour is confirmed in Task 5's browser pass.

- [ ] **Step 1: Write the component**

Create `web/src/CreditsPanel.tsx`:

```tsx
import type { CSSProperties } from 'react'
import { thumbUrl } from './api'

// Same shell as `NotePanel`'s, deliberately: two panels that open over the
// page with different keys should not be two different-looking objects. Kept
// as its own constant rather than exported from there, because a shared style
// module for two callers is indirection without a second reader.
const panelStyle: CSSProperties = {
  position: 'absolute',
  top: 16,
  right: 16,
  width: 360,
  maxWidth: 'calc(100% - 32px)',
  maxHeight: '70vh',
  overflowY: 'auto',
  zIndex: 3,
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 12,
  borderRadius: 8,
  background: 'rgba(24, 24, 24, 0.94)',
  color: 'rgba(255, 255, 255, 0.92)',
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
}

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 10,
  padding: '6px 0',
  border: 'none',
  borderTop: '1px solid rgba(255, 255, 255, 0.12)',
  background: 'none',
  color: 'inherit',
  font: 'inherit',
  textAlign: 'left',
  cursor: 'pointer',
  width: '100%',
}

// A credits card is recognisable at a glance and unrecognisable from its page
// number alone, which is the whole reason the list carries thumbnails. Small,
// because the list is short — four pages on the primary corpus volume — and
// this is for telling a donation card from a staff page, not for reading it.
const thumbStyle: CSSProperties = {
  height: 56,
  width: 'auto',
  objectFit: 'contain',
  display: 'block',
  flex: '0 0 auto',
  background: 'rgba(0, 0, 0, 0.35)',
}

export interface CreditsPanelProps {
  visible: boolean
  work: string
  /** The edition being read. The list is its credit pages, not the work's. */
  edition: string
  /**
   * Zero-based page indices, straight from `Resolver.credits` — the sidecar's
   * own unit, the same kind of value `spreads` holds. Converted to 1-based
   * only in the label below, which is the boundary convention `altAt` and
   * `Gap`'s page numbers already follow.
   */
  pages: number[]
  /** Whether `skip` is on, so the panel can say what the list currently means. */
  skipping: boolean
  /** Take the reader to a page. Zero-based; `App.tsx` converts to a slot. */
  onGo: (page: number) => void
  onClose: () => void
}

/**
 * The pages a scanlation group asked for in exchange for the work, gathered in
 * one place.
 *
 * **Set aside, never hidden.** `modules/credits.ts` is normative: a group's
 * credit is what it asked for, so `skip` keeps those pages out of the reading
 * flow and this is the section of their own they go to. Taking an entry works
 * whether or not `skip` is on — nothing here is meant to become unreachable,
 * and a reader who lands on a skipped page steps off it with the next arrow
 * (`pairing.ts`'s `nearestGroup`).
 *
 * **No labels.** The sidecar records page indices and nothing else. Which of
 * these is a donation card and which is a staff-credits page is something
 * `docs/status.md` §4 established by looking at them — four pages on the
 * primary volume, three the same donation card and one naming three people —
 * not something the format carries. A panel that named them would be inventing
 * a distinction the document cannot support, so it shows the page and lets a
 * reader see.
 *
 * The empty case is a sentence rather than an empty box, following
 * `chapterLine`'s rule that "an empty readout reads as a bug, a labelled one
 * reads as a fact about this particular work". It is the common case: a raw
 * edition has no scanlation credits at all, and no sidecar in the corpus has
 * had `taiyaku credits --write` run against it yet.
 */
export function CreditsPanel({
  visible,
  work,
  edition,
  pages,
  skipping,
  onGo,
  onClose,
}: CreditsPanelProps) {
  if (!visible) return null
  return (
    <div style={panelStyle} role="dialog" aria-label={`Credit pages in ${edition}`}>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
        <strong style={{ flex: 1 }}>Credit pages — {edition}</strong>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
      {pages.length === 0 ? (
        <p style={{ margin: 0, opacity: 0.8 }}>No credit pages found in {edition}.</p>
      ) : (
        <>
          {/* What the setting beside this panel is currently doing, said where
              the reader is looking at its consequences rather than only in the
              settings panel two keys away. */}
          <p style={{ margin: 0, opacity: 0.8 }}>
            {skipping
              ? 'These are kept out of the reading flow. Taking one still goes there.'
              : 'These are still in the reading flow. Switch on “skip” in settings to set them aside.'}
          </p>
          {pages.map((page) => (
            <button key={page} type="button" style={rowStyle} onClick={() => onGo(page)}>
              <img src={thumbUrl(work, edition, page)} alt="" style={thumbStyle} />
              {/* 1-based, the one conversion in this file: read by a person,
                  the same category `Gap`'s page numbers fall into. `page`
                  itself stays zero-based for `thumbUrl` and `onGo`, which is
                  what the server and the resolver expect. */}
              <span>page {page + 1}</span>
            </button>
          ))}
        </>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Verify and commit**

```sh
cd web && npm run typecheck && npm run lint && npm test
```

Expected: typecheck clean, lint clean, every existing test still PASS (no test
covers this file; the run confirms nothing else broke).

```sh
git add web/src/CreditsPanel.tsx
git commit -m "a panel for the pages set aside"
```

---

### Task 4: Wire it into `App.tsx`

**Files:**
- Modify: `web/src/App.tsx`

**Interfaces:**
- Consumes: `creditSlotsIn` (Task 1), `readableGroups` (Task 2), `CreditsPanel`
  (Task 3).
- Produces: nothing later tasks call.

- [ ] **Step 1: Imports**

Add to the existing imports, matching their current grouping and order:

```ts
import { CreditsPanel } from './CreditsPanel'
import { creditSlotsIn, creditsModule, PANEL_INTENT } from './modules/credits'
```

and add `readableGroups` to the **existing** `./pairing` import, which today
reads:

```ts
import { cellsFor, correctionTiles, groups, type Group, type Mode } from './pairing'
```

Do not add a second import statement from `./pairing`. `stepToGroup` is **not**
imported here and does not need to be — it is called from `session.ts`'s
`applyIntent`, which receives `groupedRef.current`, so Task 2's change reaches
the arrow keys through the array this task filters, with no import here.

- [ ] **Step 2: Panel visibility state**

Beside `const [settingsVisible, setSettingsVisible] = useState(false)`:

```ts
  // `C`'s target (`modules/credits.ts`), the same shape `settingsVisible` has
  // and for the same reason: panel visibility is UI state with no bearing on
  // `ReadingState`, so it never reaches `applyIntent`.
  const [creditsVisible, setCreditsVisible] = useState(false)
```

- [ ] **Step 3: The module-switch case**

In the `intent.kind === 'module'` switch, after the existing cases:

```ts
          case PANEL_INTENT: {
            // Gated on the preference here rather than inside the component,
            // so a reader who has switched the panel off pays no render and
            // `C` is genuinely inert — which is what "Offer the panel that
            // lists them" means.
            if (!creditsPrefsRef.current.panel) return
            setCreditsVisible((shown) => !shown)
            break
          }
```

**This needs a ref, because the keydown handler subscribes once at load and so
closes over the first render's `prefsMap`.** That is the same problem
`groupedRef` and `hereRef` already solve, and this follows them exactly —
declared beside them, assigned during render:

```ts
  // The credits preferences, for the keydown handler — which subscribes once
  // at load and so cannot see `prefsMap`. Assigned during render like
  // `groupedRef`, and for the same reason: it is derived state, recomputed
  // from props and state every render, so a discarded render writes the same
  // value the retained one will.
  const creditsPrefsRef = useRef<CreditsPrefs>(creditsModule.defaults)
```

and, beside `groupedRef.current = grouped`:

```ts
  creditsPrefsRef.current = creditsPrefs
```

`CreditsPrefs` comes from `./modules/credits`. Do **not** reach for
`prefsMapRef` — no such ref exists, and adding one to read a single slice would
be a wider mechanism than this needs.

- [ ] **Step 4: Derive the skipped slots and filter the grouping**

`slots` and `grouped` are computed together after the `at` narrowing. Replace
the `grouped` line, keeping `slots` and `effectiveReading` as they are:

```ts
  const creditsPrefs = sliceFor(prefsMap, creditsModule)
  // Page indices from the sidecar, converted to this edition's slots exactly
  // once per render. Empty — and so free — whenever `skip` is off or the
  // sidecar names no credit pages, which is every edition in the corpus today.
  const skippedSlots = creditsPrefs.skip
    ? new Set(creditSlotsIn(slots, resolver.credits(shown.edition)))
    : EMPTY_SLOTS
  // One filter, three surfaces: this array is what the page area draws, what
  // `groupedRef` gives the arrow keys, and what `Chrome` hands the filmstrip.
  // Filtering here is what makes "out of the reading flow" mean all three
  // rather than the arrows alone.
  const grouped = readableGroups(
    groups(slots, mode, effectiveReading, regroupPrefs.breaks[shown.edition] ?? []),
    skippedSlots,
  )
```

and at module scope, beside the other file-level constants:

```ts
/** One empty set rather than a fresh one per render, so `readableGroups`' identity fast path is actually reached. */
const EMPTY_SLOTS: Set<number> = new Set()
```

- [ ] **Step 5: Mount the panel**

After `<Settings … />`, inside the same root element:

```tsx
      <CreditsPanel
        visible={creditsVisible}
        work={work}
        edition={shown.edition}
        pages={resolver.credits(shown.edition)}
        skipping={creditsPrefs.skip}
        onGo={(page) => {
          // The panel deals in pages, the reader in slots. `slots` is this
          // edition's own table, so the first slot presenting the page is
          // where to land — a joined spread's two slots are one image, and
          // its first is the one `groups` puts first.
          const slot = slots.findIndex((position) => position.page === page)
          if (slot === -1) return
          goToSlot(slot)
          setCreditsVisible(false)
        }}
        onClose={() => setCreditsVisible(false)}
      />
```

**Mounted unconditionally, not under `at`.** A gap has a viewed edition and
that edition has credit pages, so the list is answerable there — and this is
the single mount site O25 left, which exists so that a panel added later is
reachable in a gap because there is nowhere else to put it.

- [ ] **Step 6: Verify**

```sh
cd web && npm run typecheck && npm run lint && npm test
```

Expected: typecheck clean, lint clean, every test PASS. No test covers
`App.tsx`, so the count should be unchanged from Task 2's run.

- [ ] **Step 7: Commit**

```sh
git add web/src/App.tsx
git commit -m "both credit-page toggles now govern something"
```

---

### Task 5: Build a fixture, drive it, and correct §4

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/notes.md`

- [ ] **Step 1: Build the fixture — the corpus has no credits to show**

**No corpus sidecar has a `credits` field.** Read directly: all five editions
across `corpus/gotoubun-v01` and `corpus/kaguya-v01` report `credits: None`. So
`Resolver.credits` returns `[]` everywhere, the panel renders its empty state,
and `skip` skips nothing — **a pass against the corpus as it stands cannot
fail.** Build one on a copy, never on the corpus's own file:

```sh
cd /home/davey/projects/taiyaku
SCRATCH=/tmp/claude-1000/-home-davey-projects-taiyaku/…/scratchpad   # this session's
cp corpus/gotoubun-v01/taiyaku.json "$SCRATCH/credits-fixture.json"
cargo build --release -p taiyaku-cli   # the binary is taiyaku-cli's, not taiyaku-serve's
```

Read the member paths out of the sidecar rather than retyping them:

```sh
python3 -c "
import json; d=json.load(open('corpus/gotoubun-v01/taiyaku.json'))
print('\n'.join(d['editions']['en-fan']['members']))"
```

Then, with those paths (relative to the corpus root):

```sh
./target/release/taiyaku credits <each en-fan member> \
  --sidecar "$SCRATCH/credits-fixture.json" --write
```

Confirm it wrote the four the gated corpus check pins:

```sh
python3 -c "
import json; d=json.load(open('$SCRATCH/credits-fixture.json'))
print(d['editions']['en-fan'].get('credits'))"
```

Expected: `[95, 168, 198, 225]`, matching
`gotoubun_fan_edition_classifies_four_credit_pages`'s
`assert_eq!(pages, vec![95, 168, 198, 225])`. **These are zero-based page
indices** — `credit_pages` reads run geometry — so the panel must show pages
**96, 169, 199 and 226**. None is a joined spread (`en-fan`'s spreads are 45,
91, 133, 164, 196), so each is one slot.

If the numbers differ, stop: either the rule has changed or the wrong members
were passed, and the rest of this pass would be measuring the wrong thing.

- [ ] **Step 2: Serve the fixture and open the reader**

```sh
./target/release/taiyaku serve "$SCRATCH/credits-fixture.json" --addr 127.0.0.1:8791
cd web && TAIYAKU_SERVE=http://127.0.0.1:8791 npm run dev
```

The corpus's own `taiyaku.json` is never served here, so nothing this pass does
can write to it. Stop both servers by **PID** afterwards — this machine runs
other projects' dev servers, and a pattern kill takes them out.

- [ ] **Step 3: Walk the seven checks**

Record what each produced. All seven must hold.

1. Viewing `en-fan`, `C` opens the panel and it lists **exactly four** entries,
   each with a thumbnail, reading `page 96`, `page 169`, `page 199`, `page 226`.
2. Viewing `raw-mono`, the panel reads `No credit pages found in raw-mono.`
3. With `skip` **off**: the filmstrip shows all **235** `en-fan` tiles in
   `single` mode, and arrowing through page index 95 lands on it.
4. With `skip` **on**: the strip shows **231**, and the arrow that would have
   landed on page index 95 lands on the next readable page instead.
5. The position readout's total is **unchanged** by the toggle — still `/ 240`.
   This is "skipping never renumbers" on screen.
6. Taking the panel's `page 96` entry lands there **with `skip` still on**, and
   the next arrow leaves — forward and backward both.
7. Switching `panel` off in settings makes `C` do nothing.

Then switch to `two` mode with `skip` on and confirm the mixed-group rule: a
group pairing a credit page with an ordinary one is **kept**, so the ordinary
page is still reachable.

- [ ] **Step 4: Record the measurement in `notes.md`**

`status.md`'s preamble promises every claim there traces to a measurement in
`notes.md`. Add a section naming what was driven, against which fixture, and
what each of the seven checks produced — including the fixture-building step,
because "the corpus had no credits to show" is the finding that shaped this
pass and the next person needs it.

- [ ] **Step 5: Correct `status.md` §4 — the `nextReadable` sentence is now wrong**

§4 currently reads: "the readout jumps rather than counting differently.
Recorded on `web/src/modules/credits.ts`'s `creditsModule`, and carried out by
`nextReadable`, which walks to the next unskipped slot without renumbering."

The first half stands. The mechanism named in the second half does not. Rewrite
it to name `readableGroups` and say why the slot walker did not survive —
arrows have stepped by group since `c11c1e9`, and a slot walker governs arrows
alone while a group filter governs the page area, the arrows and the filmstrip
together. Follow this branch's convention for corrections: say what the
document previously claimed and why it was wrong, rather than quietly editing.

Also update §4's closing paragraph, which says the reader's half has not
shipped and names `creditSlots` as "the other thing to finish". Both are now
done.

- [ ] **Step 6: Update the test counts, and check them**

The `web/` figure appears twice near the top of `status.md`. Read the new count
off the actual run rather than adding up by hand:

```sh
cd web && npm test 2>&1 | grep -E "Tests "
```

No Rust tests change, so "Five crates, 410 tests" and "Twelve of the 410 are
standing checks" both stand.

- [ ] **Step 7: Sweep, commit, and open the PR**

```sh
cd /home/davey/projects/taiyaku
grep -rn "nextReadable\|creditSlots\b" docs/ web/src/   # expect nothing
grep -n "397 in \`web" docs/status.md                    # expect the new number instead
```

```sh
git add docs/status.md docs/notes.md
git commit -m "docs: the credit-pages panel ships, and §4's mechanism is corrected"
```

The remote is Gitea and `gh` is authenticated against the wrong host — use the
`gitea` MCP tools. **This branch is stacked on `o25-chrome-in-a-gap` (PR #16),
which is unmerged**, so open this PR against `o25-chrome-in-a-gap`, not `main`;
once #16 squashes, rebase with `git rebase --onto main o25-chrome-in-a-gap
credits-panel` and retarget.

---

## What this plan does not do

- **No labels or classification in the panel.** The sidecar records page
  indices; anything more would be invented.
- **No renumbering, and no change to the position readout's total.**
- **No change to `taiyaku credits` or the rule behind it.** Recall is bounded
  above rather than known — `c000`'s page 43 is a credits page the solver
  *paired*, so no unpaired-based signal reaches it — and that is a CLI question.
- **Nothing about the known false positive.** Kaguya's page 212 is a colour
  announcement the rule classifies and a corpus check pins deliberately. The
  panel will list it if that sidecar is ever written: the panel shows what the
  sidecar says.

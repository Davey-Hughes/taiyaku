import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import { rules, selectors } from '../testing/css'

/**
 * The Rules screen read from its stylesheet — spec 2026-09-17 §10.
 *
 * Only facts that are *defects* if they regress live here. How the screen
 * looks is the browser pass's job (§5 item 2); what this pins is the
 * handful of declarations whose absence is a bug rather than a preference.
 */

/**
 * The bodies of every rule whose selector list *contains* `selector`.
 *
 * **Not an exact prelude match**, unlike `reader.css.test.ts`'s helper of
 * the same name: several rules below group two selectors — the sticky
 * `th:first-child` and `td:first-child` share one body — and an exact match
 * on the whole prelude silently finds none of them, returning `[]` and
 * passing any assertion written as `toEqual([])`.
 */
const bodies = (selector: string) =>
  rules(adminSource)
    .filter((rule) => selectors(rule.prelude).includes(selector))
    .map((rule) => rule.body)

/** Every value `property` is given in those rules, whitespace removed. */
function declared(selector: string, property: string): string[] {
  return bodies(selector).flatMap((body) =>
    [
      ...body.matchAll(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`, 'g')),
    ].map((m) => m[1].replace(/\s+/g, '')),
  )
}

describe('the rule list', () => {
  it('prints each ordinal once', () => {
    // `RuleList` renders an `<ol>` *and* a `.ordinal` span, so the browser's
    // own marker drew `1. 1`, `2. 2` down the whole list.
    expect(declared('.rule-list ol', 'list-style')).toEqual(['none'])
  })

  it('says the row can be dragged, and stops saying it while a write is', () => {
    // §10.4 commits ⑤a to "the drag handle gets an affordance it currently
    // lacks"; the plan narrowed away from it and the whole-branch review
    // found it missing. On the row rather than a grip, because `RuleList`
    // puts `draggable` on the whole `<li>` — a grip would picture a control
    // that is not there.
    expect(declared('.rule-row', 'cursor')).toEqual(['grab'])
    // **Hooked on `draggable`, the attribute `RuleList` actually sets.** The
    // first draft of this rule used `[aria-disabled="true"]`, which that
    // component never sets on the row — it would have styled a state that
    // cannot occur. Verified in a browser: the row renders
    // `draggable="true"`, carries no `aria-disabled`, and flipping the
    // attribute moves the cursor grab -> default.
    expect(declared('.rule-row[draggable="false"]', 'cursor')).toEqual([
      'default',
    ])
  })

  it('marks the selected row without moving the column beside it', () => {
    // An inset shadow, not a border: a border that exists on one row and
    // not the next shifts that row's content by its width. The same reason
    // `.proposal-list li.strong` uses one.
    const selected = declared('.rule-row[aria-current="true"]', 'box-shadow')
    expect(selected).toHaveLength(1)
    expect(selected[0]).toContain('inset')
    expect(selected[0]).toContain('var(--accent)')
    expect(declared('.rule-row[aria-current="true"]', 'border-width')).toEqual(
      [],
    )
  })
})

describe('the tab look', () => {
  it('underlines the current tab without moving the row', () => {
    // `inset 0 -2px 0`, not a border: ②'s `.library-chips` draws it
    // this way so selecting a tab does not shift the row's baseline.
    for (const prelude of [
      '.preview-tabs button[aria-current="true"]',
      '.config-nav a[aria-current="page"]',
    ]) {
      const shadow = declared(prelude, 'box-shadow')
      expect(shadow, prelude).toEqual(['inset0-2px0var(--accent)'])
      expect(declared(prelude, 'border-bottom-width'), prelude).toEqual([])
    }
  })
})

describe('the field pattern', () => {
  it('is a shared class, scoped to the two screens that opted in', () => {
    // C3 built it for AcceptForm; ⑤a's rule form edits the same content, so
    // it is hoisted rather than copied — but bare, a hoisted `.field` also
    // reached `MetadataEditor.tsx`'s Series-page label, which never opted
    // in. The whole-branch review's I1 scoped it to the two screens that did.
    expect(bodies('.field')).toHaveLength(0)
    expect(bodies('.proposal-detail .field')).toHaveLength(1)
    // **Both screens, because the name says two.** This asserted only the
    // inbox until the re-review: dropping `.rule-form .field,` from the
    // shared prelude left every gate green while the rule form's nine labels
    // fell back inline beside the previous field's input — the predecessor
    // defect this whole block exists to prevent. Two bodies: the shared one
    // and `flex: 1 1 8rem`.
    // **The same body, not a count.** `toHaveLength(2)` was the first form
    // and it breaks on a legitimate third rule — a narrow-width override is
    // exactly the kind of edit this branch has been making — while still
    // passing if the shared prelude were split into two identical standalone
    // rules, which is the thing being guarded against.
    expect(bodies('.rule-form .field')).toContain(
      bodies('.proposal-detail .field')[0],
    )
  })

  it('keeps the pattern field on a row of its own, on both screens that use it', () => {
    // Measured in C3: sharing the row gave the pattern 416px of 807 at a
    // 1100px window and cut a 52-character rel_path prefix. Two selectors,
    // one body: C1 raised `.rule-form .field.pattern`'s specificity to beat
    // `.rule-form .field` (below), which tied it at (0,2,0) and won on
    // source order until then.
    expect(declared('.proposal-detail .field.pattern', 'flex')).toEqual([
      '10100%',
    ])
    expect(declared('.rule-form .field.pattern', 'flex')).toEqual(['10100%'])
  })

  it('draws the rule form’s controls through it', () => {
    expect(bodies('.rule-form')).toHaveLength(1)
  })
})

describe('the preview tables', () => {
  it('gives a sticky first cell its own opaque background', () => {
    // A sticky cell with no background of its own lets the cells it slides
    // over show through it — the position is right and the reading is
    // unusable, which no geometry assertion can see.
    for (const prelude of [
      '.preview .table-scroll th:first-child',
      '.preview .table-scroll td:first-child',
    ]) {
      expect(declared(prelude, 'position'), prelude).toEqual(['sticky'])
      expect(declared(prelude, 'left'), prelude).toEqual(['0'])
      expect(declared(prelude, 'background'), prelude).toHaveLength(1)
      // **No `max-width` assertion, and that is the finding rather than an
      // omission.** This rule carried `max-width: 16rem` until it was
      // measured: the pinned cell renders **35px** wide in a 390px box, with
      // the cap and without it alike, because `overflow-wrap: anywhere`
      // below collapses its min-content and the auto-layout table gives it
      // what is left. A 256px cap over a 35px cell constrains nothing. It
      // was pinned here for one commit, which would have frozen a
      // declaration that does not do anything.
    }
  })

  it('wraps the pinned path rather than cutting its end off', () => {
    // The end is what tells these rows apart. Against the library this was
    // built on, all 1,915 file paths are distinct in full and only 762 stay
    // distinct once cut to 36 characters (a sharp edge — 757 at 35, 824 at
    // 37; see `admin.css` for why 36) — so an ellipsis would have
    // made 1,153 of them, 60%, read identically in the one column that is
    // pinned so a reader can tell which row they are on.
    for (const prelude of [
      '.preview .table-scroll th:first-child',
      '.preview .table-scroll td:first-child',
    ]) {
      expect(declared(prelude, 'white-space'), prelude).toEqual(['normal'])
      // A release filename has no spaces to break at, so the cell needs
      // this to wrap at all — without it one long token paints past the cell
      // and over the columns sliding underneath.
      expect(declared(prelude, 'overflow-wrap'), prelude).toEqual(['anywhere'])
      expect(declared(prelude, 'text-overflow'), prelude).toEqual([])
      // **And a floor, because wrapping without one hides the path just as
      // well.** The auto table gave this column 38px at 390 — three
      // characters — and a 67-character path wrapped into that stood 933px
      // tall, taller than the viewport, over a 161,451px table. 10rem is
      // where the median row stops improving (933 -> 93 -> 72, then 72 at
      // 12 and 14rem) while the box's own scroll keeps growing.
      expect(declared(prelude, 'min-width'), prelude).toEqual(['10rem'])
    }
  })
})

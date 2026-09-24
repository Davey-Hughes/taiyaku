# Style pass ⑤b — the Configuration screens: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Configuration screens — roots, the scan report, users and backup — say what they mean: three kinds of form control stop sharing one grid track, every card gets a measure, `--danger` reaches the two removal blocks, a root states itself as a labelled list, and a zero recedes.

**Architecture:** Most of the work is new rules in the existing `admin.css` tail (lines 1140–1415), which is where all four screens already live. Three markup changes are needed and are taken deliberately (spec §11.8): a `<dl>` for the root's facts, one `data-zero` attribute on scan-report rows, and moving the checkbox rows and `p.why` out of `.fields`. The floor gate in `base.css.test.ts` is widened to two shapes it cannot see today.

**Tech Stack:** React 19, TypeScript, plain CSS with custom properties, Vitest across three projects — `node` (`src/**/*.test.ts`), `jsdom` (`src/**/*.test.tsx`, excluding `*.browser.test.tsx`), `browser` (`src/**/*.browser.test.tsx`, real Chromium).

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-09-17-style-pass-design.md` §11. §5's checklist applies to this PR as to every other in the pass.
- **This branch must be rebased onto `main` after #173 merges.** Measured, not assumed — see "The one ordering constraint" below. Task 8 is red until #173 is in.
- **Colour comes from `styles/tokens.css` only.** §4 rule 2. `colourLiterals.test.ts` fails on any hex, `rgb()`/`rgba()`/`hsl()`/`hsla()` or CSS system-colour keyword added under `src/`, and its allowance only shrinks — do not add an entry.
- **No `opacity` for quiet text.** Quiet is bought with `--muted`, size and border. The one exemption is a disabled control.
- **Spacing, type and radius come from tokens**: `--space-1: 4px` … `--space-7: 48px`, `--text-h1`/`h2`/`h3`/`body`/`ui`/`meta`, `--radius-control: 2px`. Colour tokens: `--bg --surface --field --line --line-strong --text --muted --accent --on-accent --danger --success --warning`.
- **Run the web gate with the thread throttle**: `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh`. An unthrottled run has been killed for memory on this machine.
- **Verify a gate by its own exit status.** Never `| tail`, never `; echo` — both report the *pipeline's* status. Write the run to a file and read `$?` from it.
- **Vitest reporter**: under Claude Code vitest picks an `agent` reporter that hides passing tests' console output. Use `--reporter=default` when you need to see it.
- **Every new test is watched failing** with its defect put back by hand before it counts as written.
- **No AI/assistant attribution** in any commit message, comment or doc.

---

## The one ordering constraint

**Task 8 (the gate widening) cannot go green until #173 is merged.** This was measured, not reasoned about: a simulation of the widened gate was run against this branch's cascade and against the same cascade with #173's `reader.css` applied.

| cascade | uncovered by the widened gate |
|---|---|
| this branch (`149a5b49`) | 3 — `.account input`, `.reader-dialog-panel :where(input)`, `.reader-dialog-panel :where(select)` |
| with #173's `reader.css` | **1** — `.account input` |

#173 adds the companion `.reader-dialog-panel :where(input, select, textarea)`, whose selector text differs from the exposed `.reader-dialog-panel :where(input, select)` by one name. Today's gate pairs rules by *exact selector text*, so that difference would matter — Task 8's expansion is what makes the superset pair correctly, and the table above is the proof it does.

**So: rebase onto `main` once #173 lands, then do Task 8.** Tasks 1–7 are independent of it and can proceed now.

---

## What the widened gate newly catches, and why it is ⑤b's

`.account input { font: inherit }` (`admin.css:1380`) is the one genuinely new finding. It is **not a live defect** — `.account` sets no `font-size`, so the input inherits 16 px from `body` — but it is unprovable from the text, it outranks `base.css`'s zero-specificity floor at (0,1,1), and it is the exact shape that bit the reader in O151. It gets a companion in Task 8.

`.account` is not one of ⑤b's four screens. It is taken here because the gate that flags it is ⑤b's, and leaving a known-red gate for ⑤c is not an option.

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `web/src/config.ts` | Adds `regimeFacts` beside `regimeLine` — a pure function over `Root`/`Probe`, testable in the `node` project | 4 |
| `web/src/RootRow.tsx` | Renders the facts as a `<dl>`; nothing else changes | 4 |
| `web/src/ScanReport.tsx` | `Count` and `Paths` mark a zero row with `data-zero`; the `refused` row does too | 5 |
| `web/src/UserRow.tsx` | Checkbox rows and `p.why` move out of `.fields`; `.field` reaches the Role and password labels | 1, 2 |
| `web/src/Users.tsx` | The add-account form's labels take `.field` | 1, 2 |
| `web/src/Roots.tsx` | The add-root form's labels take `.field` | 1, 2 |
| `web/src/styles/admin.css` | Every new rule, in the existing 1140–1415 tail; the five `color-mix` and nine `opacity` removals | 1–8 |
| `web/src/styles/base.css.test.ts` | The floor gate learns the `font` shorthand and a `:where()`-wrapped subject | 8 |
| `web/src/styles/admin-config.css.test.ts` | **New**, `node` project: pins the pre-token sweep and the measure | 3, 7 |
| `web/src/Config.browser.test.tsx` | **New**, `browser` project: the measure and no sideways scroll at 390, on the document *and* on `main` | 7 |

---

## Task 1: Three kinds stop sharing one grid track

`.fields` is `repeat(auto-fit, minmax(14rem, 1fr))` and is written **twice**, differing only in a `margin-top`, at `admin.css:1161` (`.root-row .fields`) and `admin.css:1282` (`.user-row .fields`). (This said "byte-identical"; `.user-row .fields` has carried that margin since before the branch, so it never was.) On `/config/users` its five children are `label` (Role select), `p.why`, and three checkbox `label`s — a paragraph and three checkboxes occupying field slots.

There is a second defect here the spec did not name: **`.user-row .fields label` has no rule at all**, so `<span>Role</span><select>` is inline flow and "Role" butts against the select, exactly like the password field one screen down. Applying `.field` fixes both in one move.

**Files:**
- Modify: `web/src/UserRow.tsx:372-430` (checkbox rows and `p.why` leave `.fields`), `web/src/UserRow.tsx:465-475` (the password label)
- Modify: `web/src/Users.tsx:153-180`, `web/src/Roots.tsx` (add-root labels)
- Modify: `web/src/styles/admin.css` (the two `.fields` rules)
- Test: `web/src/UserRow.test.tsx`

**Interfaces:**
- Consumes: `.field`, hoisted by ⑤a and scoped at `admin.css:493-494` to `.proposal-detail .field, .rule-form .field`. This task adds `.root-row .field, .user-row .field, .add-user .field, .add-root .field` to that selector list.
- Produces: the class names `.checks` (the full-width checkbox group) and `.field` on every labelled control on these four screens. Tasks 2, 3 and 7 rely on both.

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/UserRow.test.tsx — append inside the existing describe
it('keeps the role explanation out of the field grid', () => {
  const host = render(<UserRow user={adminUser} {...noopHandlers} />)
  const fields = host.querySelector('.user-row .fields')
  expect(fields).not.toBeNull()
  // `p.why` is a sentence, not a field: it must not be a child of the grid.
  expect(fields?.querySelector('p.why')).toBeNull()
  // It still renders — it moved, it did not go.
  expect(host.querySelector('.user-row p.why')).not.toBeNull()
})

it('groups the three permission checkboxes in a row of their own', () => {
  const host = render(<UserRow user={adminUser} {...noopHandlers} />)
  const checks = host.querySelector('.user-row .checks')
  expect(checks).not.toBeNull()
  expect(checks?.querySelectorAll('input[type="checkbox"]')).toHaveLength(3)
  // And none of them is left behind in the grid.
  expect(
    host.querySelectorAll('.user-row .fields input[type="checkbox"]'),
  ).toHaveLength(0)
})

it('gives the role label the field pattern rather than inline flow', () => {
  const host = render(<UserRow user={adminUser} {...noopHandlers} />)
  const role = host.querySelector('select[name="role"]')?.closest('label')
  expect(role?.className).toContain('field')
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project jsdom src/UserRow.test.tsx --reporter=default
```
Expected: all three FAIL — `.checks` is null, `p.why` is found inside `.fields`, the role label has no class.

- [ ] **Step 3: Move the markup**

```tsx
// web/src/UserRow.tsx — replace the <div className="fields"> block
<div className="fields">
  <label className="field">
    <span>Role</span>
    <select
      name="role"
      value={shown.role}
      disabled={saving || isSelf || lastAdmin}
      onChange={(e) => save({ role: e.target.value })}
    >
      {ROLES.map((role) => (
        <option key={role} value={role}>
          {role}
        </option>
      ))}
    </select>
  </label>
</div>
{roleWhy !== undefined && <p className="why">{roleWhy}</p>}

<div className="checks">
  <label>
    <input
      type="checkbox"
      name="can_download"
      checked={shown.can_download}
      disabled={saving}
      onChange={(e) => save({ can_download: e.target.checked })}
    />
    <span>May download whole editions</span>
  </label>
  <label>
    <input
      type="checkbox"
      name="can_stream"
      checked={shown.can_stream}
      disabled={saving}
      onChange={(e) => save({ can_stream: e.target.checked })}
    />
    <span>May stream pages</span>
  </label>
  <label>
    <input
      type="checkbox"
      name="can_stamp"
      checked={shown.can_stamp}
      disabled={saving}
      onChange={(e) => save({ can_stamp: e.target.checked })}
    />
    <span>May write ComicInfo.xml into archives</span>
  </label>
</div>
```

Check the third checkbox's `name` and label against the file before pasting — the block above reproduces the first two verbatim and the third is written from the screen.

- [ ] **Step 4: Add `className="field"` to the remaining labelled controls**

In `web/src/UserRow.tsx`, the password label:

```tsx
<label className="field">
  <span>Set a new password</span>
  <input
    type="password"
    name="password"
    value={password}
    disabled={isSelf || saving}
    onChange={(e) => setPassword(e.target.value)}
  />
</label>
```

In `web/src/Users.tsx`, **all three** `<label>` elements inside
`<form className="add-user">` — Name, Password **and Role**. The Role one is
a `<select>`, so `.add-user .field select` must also join the control reset
and the coarse-pointer companion in Step 5, or the same control looks one way
on the user row and another on the add form. In `web/src/Roots.tsx`, every `<label>` inside the add-root form.

**And in `web/src/RootRow.tsx`, all three `<label>` shapes inside
`<div className="fields">`** — the multiline (textarea) branch, the
single-line (input) branch, and the scan-interval number box. Step 5 adds
`.root-row .field*` to all three selector lists, and without this they match
nothing. **Delete `.root-row .fields label` (`admin.css:1192`) at the same
time**: it is (0,2,1) against `.field`'s (0,2,0) and would win, exactly like
the two add-form rules below.

Leave the `autoCapitalize`/`autoCorrect`/`spellCheck` attributes exactly as
they are — they fix a reported defect.

- [ ] **Step 5: Extend all *three* `.field` selector lists**

`.field` is a micro-label pattern: it sets `font-size: 0.7rem`,
`letter-spacing: 0.06em` and `text-transform: uppercase` on the **label**,
and all three of those are inherited properties that reach the control
inside it. ⑤a's `.field input, .field select` rule at `admin.css:529` is the
reset that undoes them, and the `@media (pointer: coarse)` block at
`admin.css:566` is the companion that keeps the reset's 14 px off the iOS
zoom threshold. **Adding a host to the first list and not the other two
renders every control on these screens at 11.2 px, uppercase, with no border
— and puts it under the floor.** All three, or none.

```css
/* admin.css:493 — the label pattern */
.proposal-detail .field,
.rule-form .field,
.root-row .field,
.user-row .field,
.add-user .field,
.add-root .field {
  min-width: 0;
  display: grid;
  gap: 0.15rem;
  font-size: 0.7rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
```

```css
/* admin.css:529 — the control reset. `textarea` is new to this list: the
   root row's `exclude_globs` box is one, and the four existing hosts have
   none, which is why it was never needed before. */
.proposal-detail .field input,
.rule-form .field input,
.proposal-detail .field select,
.rule-form .field select,
.root-row .field input,
.root-row .field select,
.root-row .field textarea,
.user-row .field input,
.user-row .field select,
.add-user .field input,
.add-root .field input,
.add-root .field textarea {
  /* unchanged body — do not retype it, add the selectors above it */
}
```

```css
/* admin.css:566 — the coarse-pointer companion, which must name exactly the
   same selectors. `base.css.test.ts` pins that these two never drift apart,
   and after Task 8 it will see a miss here that it cannot see today. */
@media (pointer: coarse) {
  .proposal-detail .field input,
  .rule-form .field input,
  .proposal-detail .field select,
  .rule-form .field select,
  .root-row .field input,
  .root-row .field select,
  .root-row .field textarea,
  .user-row .field input,
  .user-row .field select,
  .add-user .field input,
  .add-root .field input,
  .add-root .field textarea {
    font-size: max(16px, var(--text-ui));
  }
}
```

Check the two add-forms' actual controls before pasting: this list is written
from `Users.tsx`'s two inputs and the add-root form's boxes. A selector for a
control that does not exist is harmless; a **missing** one is the defect.

**And delete the two rules `.field` replaces**, or it loses to them silently:

```css
/* admin.css:1206 — delete. `.roots .add-root label` is (0,2,1) and
   `.add-root .field` is (0,2,0), so this rule's `display: flex` would beat
   `.field`'s `display: grid` while `.field`'s inherited uppercase and 0.7rem
   still applied — the worst of both, and nothing would fail. */
.roots .add-root label {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
```

```css
/* admin.css:1340 — delete the same three declarations, keep the margin by
   moving it to the class the label now carries. */
.add-user .field {
  margin-bottom: var(--space-2);
}
```

This is the shape ⑤a's C1 was: `:where()` contributes zero specificity, and a
rule that looks like it wins can lose to one written earlier. Check it in the
browser rather than by reading — computed `display` on the label is the fact.

```css
/* admin.css — in the users section, after `.user-row .fields` */

/* A checkbox's label *is* a sentence, so it takes the row rather than a
   field track, and the box sits beside the sentence rather than under an
   uppercase micro-label. Spec §11.3. */
.user-row .checks {
  display: grid;
  gap: var(--space-2);
  margin-top: var(--space-3);
}

.user-row .checks label {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  font-size: var(--text-ui);
}

/* Prose leaves the grid and sits under the group it explains. */
.user-row > .why {
  margin: var(--space-2) 0 0;
  max-width: 60ch;
  color: var(--muted);
  font-size: var(--text-meta);
}
```

The existing `.user-row .why, .user-row .locked { font-size: 0.9em; opacity: 0.8 }` rule is replaced in Task 3, not here — leave it for now and expect the two to disagree until then.

- [ ] **Step 6: Prove the reset reached the new hosts**

```bash
cd web && npx vitest run --project node src/styles/base.css.test.ts --reporter=default
```
Expected: PASS. The floor gate already pairs every `font-size` on a control
with a coarse companion at the same selector text, so a host added to the
reset list and forgotten in the companion fails here — today, before Task 8
widens anything.

Then look at one: open `/config/users`, and confirm the Role select is not
uppercase, not 11 px, and has a border. The gate cannot see `text-transform`.

- [ ] **Step 7: Run the tests**

```bash
cd web && npx vitest run --project jsdom src/UserRow.test.tsx src/Users.test.tsx src/Roots.test.tsx --reporter=default
```
Expected: PASS, including the existing tests in all three files.

- [ ] **Step 8: Watch each new test fail against its own defect**

Put each defect back by hand, one at a time, and confirm the matching test goes red:
1. Move `<p className="why">` back inside `<div className="fields">` → the first test fails.
2. Rename `.checks` to `.fields` on the wrapper → the second fails.
3. Drop `className="field"` from the role label → the third fails.

Restore after each. A test that stays green against its own defect is not written yet.

- [ ] **Step 9: Commit**

```bash
git add web/src/UserRow.tsx web/src/Users.tsx web/src/Roots.tsx web/src/UserRow.test.tsx web/src/styles/admin.css
git commit -m "⑤b: a paragraph and three checkboxes stop occupying field slots

.field is a micro-label pattern whose uppercase and 0.7rem inherit into the
control, so all three of its selector lists gain the new hosts: the label,
the control reset, and the coarse-pointer companion that keeps the reset off
the iOS zoom threshold. textarea joins the last two for the root row's
exclude_globs box.
```

---

## Task 2: The measure

Three "Add an account" controls render **1,153 px** wide at a 1216 px `<main>`, and prose runs to roughly 180 characters a line. Two precedents already exist in this file and disagree: `.account` caps at 40 rem, `.backup .what-a-backup-is` at 60 ch. The card carries the measure; no control carries a width of its own.

**Files:**
- Modify: `web/src/styles/admin.css`
- Test: covered by Task 7's browser test (a width is not measurable in jsdom)

**Interfaces:**
- Consumes: `.field` and `.checks` from Task 1.
- Produces: `--config-measure`, a custom property on the settings area that Tasks 3 and 7 both read.

- [ ] **Step 1: Add the measure**

```css
/* admin.css — at the head of the settings-area tail, before `.roots` */

/* One measure for the settings area, set once. A width per control is a
   figure per control, and this pass has already spent four review rounds on
   that kind of drift (spec §11.4). `.account`'s 40rem is the precedent and
   the value. */
.roots,
.users,
.backup {
  --config-measure: 40rem;
}

.root-row,
.user-row,
.add-root,
.add-user,
.scan-report {
  max-width: var(--config-measure);
}

/* Prose keeps the narrower measure `.what-a-backup-is` already sets. */
.root-row .why,
.user-row > .why,
.backup .what-a-backup-is {
  max-width: 60ch;
}
```

- [ ] **Step 2: Confirm nothing sets a competing width**

```bash
cd web && grep -n "width:" src/styles/admin.css | sed -n '1,80p'
```
Expected: no `width` on a control inside `.root-row`, `.user-row`, `.add-root` or `.add-user`. If one is found, remove it in this task and say so in the commit message — the rule is that the card carries the measure.

- [ ] **Step 3: Commit**

```bash
git add web/src/styles/admin.css
git commit -m "⑤b: the card carries the measure, not the controls"
```

---

## Task 3: Empty the pre-token holdouts

Every one of `admin.css`'s remaining **5** `color-mix(in srgb, currentColor …)` borders and **9** `opacity` dims falls in this tail. Counted at `149a5b49`: `color-mix` at lines 1147, 1189, 1266, 1331, 1412; `opacity` at 1158, 1177, 1227, 1244, 1254, 1279, 1308, 1319, 1407.

**Files:**
- Modify: `web/src/styles/admin.css`
- Test: `web/src/styles/admin-config.css.test.ts` (**new**, `node` project)

**Interfaces:**
- Consumes: nothing.
- Produces: the guarantee Task 7's claim check rests on — that the sweep is asserted rather than assumed.

- [ ] **Step 1: Write the failing test**

```ts
// web/src/styles/admin-config.css.test.ts — new file
import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import { rules, withoutComments } from '../testing/css'

/**
 * ⑤b empties `admin.css` of the two pre-token idioms the style pass replaced
 * everywhere else: a border mixed from `currentColor`, and `opacity` standing
 * in for a quiet colour. Both are invisible to `colourLiterals.test.ts`,
 * which only fails on a literal — `color-mix(in srgb, currentColor 25%,
 * transparent)` names no colour at all, and `opacity: 0.8` names none either.
 *
 * Asserted here rather than left to review, because "the sweep is complete"
 * is a claim that decays the moment somebody adds a rule.
 */
describe('admin.css holds no pre-token idiom', () => {
  // The companion case — no border mixed from `currentColor` — is added by
  // Task 6, which removes the last two. Written here it would be red on
  // this commit, and a knowingly-red commit is the process failure ⑤a
  // recorded against itself.
  it('uses opacity for no quiet text', () => {
    // A disabled control is the one exemption the spec's §4 allows, and
    // `admin.css` has none today. If one is added, name it here rather than
    // widening the regex.
    const found = rules(adminSource)
      .filter(({ body }) => /(?:^|;)\s*opacity:/.test(body))
      .map(({ prelude }) => prelude)
    expect(found).toEqual([])
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project node src/styles/admin-config.css.test.ts --reporter=default
```
Expected: FAIL — 9 rules carry `opacity`.

- [ ] **Step 3: Replace the five borders**

Each `1px solid color-mix(in srgb, currentColor 25%, transparent)` becomes `1px solid var(--line)`. The two at `2px solid color-mix(in srgb, currentColor 50%, transparent)` are `.remove-root` and `.remove-user` and are **not** done here — Task 6 rewrites them with `--danger`, and doing them twice is how a figure drifts.

So in this task: lines 1147 (`.root-row`), 1266 (`.user-row`), 1412 (`.backup .what-a-backup-is` `border-top`).

```css
.root-row {
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  margin-bottom: var(--space-4);
  padding: var(--space-4);
}
```

```css
.user-row {
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  margin-bottom: var(--space-4);
  padding: var(--space-4);
}
```

```css
.backup .what-a-backup-is {
  margin-top: var(--space-5);
  border-top: 1px solid var(--line);
  padding-top: var(--space-4);
  max-width: 60ch;
}
```

- [ ] **Step 4: Replace the nine opacity dims**

Every one is quiet text, so every one becomes `color: var(--muted)`, and the three that also carry `font-size: 0.9em` take `var(--text-meta)` instead. The nine, with what each becomes:

| line | selector | becomes |
|---|---|---|
| 1158 | `.root-row .regime` | deleted — Task 4 replaces the element |
| 1177 | `.root-row .fields small` | `color: var(--muted); font-size: var(--text-meta)` |
| 1227 | `.scan-report .count dt, .scan-report .paths dt` | `color: var(--muted)` |
| 1244 | `.scan-report .unreadable-causes > dt` | `color: var(--muted)` |
| 1254 | `.scan-report .note` | `color: var(--muted); font-size: var(--text-meta)` |
| 1279 | `.user-row .why, .user-row .locked` | `color: var(--muted); font-size: var(--text-meta)` |
| 1308 | `.user-row .session-list .when, .user-row .key-list .when` | `color: var(--muted); font-size: var(--text-meta)` |
| 1319 | `.user-row.removed .gone` | `color: var(--muted)` |
| 1407 | `.backup .backup-list .when, .backup .backup-list .size` | `color: var(--muted)` |

Delete the `/* Same convention as `.root-row .regime`: opacity, not a second colour … */` comment above 1177 with the declaration it explains — it documents an idiom that no longer exists in this file.

The `.user-row .why` rule collides with the `.user-row > .why` rule Task 1 added. Keep both: the child combinator is the one that places the moved paragraph, and the descendant one still styles `.locked`. Split them:

```css
.user-row .locked {
  color: var(--muted);
  font-size: var(--text-meta);
}
```

and let Task 1's `.user-row > .why` carry the paragraph.

- [ ] **Step 5: Run the test**

```bash
cd web && npx vitest run --project node src/styles/admin-config.css.test.ts --reporter=default
```
Expected: PASS. Three of the five `color-mix` borders are gone too, but nothing asserts that yet — Task 6 removes the last two and adds the assertion in the same commit, so no commit on this branch is knowingly red.

- [ ] **Step 6: Watch the test fail against its own defect**

Put `opacity: 0.8` back on `.scan-report .note` and confirm the second test goes red naming that selector. Restore.

- [ ] **Step 7: Commit**

```bash
git add web/src/styles/admin.css web/src/styles/admin-config.css.test.ts
git commit -m "⑤b: the tail of admin.css takes tokens

The opacity sweep is complete and asserted. Three of the five color-mix
borders go with it; the last two are .remove-root and .remove-user, which
Task 6 rewrites with --danger rather than tokenising a treatment C4
section 2 already judged insufficient. The assertion that none is left
lands with that task, so this commit asserts only what it finished."
```

---

## Task 4: A root states itself as a labelled list

`config.ts:370`'s `regimeLine` joins **three** facts with `·` — watch, interval, last scan — and the fourth separator is inside the third, which appends `· ok`. None is ever absent: every arm has a fallback string.

**Files:**
- Modify: `web/src/config.ts` (replace `regimeLine` with `regimeFacts`)
- Modify: `web/src/RootRow.tsx:439`
- Modify: `web/src/styles/admin.css`
- Test: `web/src/config.test.ts`, `web/src/RootRow.test.tsx`

**Interfaces:**
- Consumes: `Root` and `Probe` from `config.ts`, already imported by `RootRow.tsx`.
- Produces: `export function regimeFacts(root: Root, probe?: Probe): Array<{ label: string; value: string }>` — exactly three entries, in the order watch, scan every, last scan. Task 7's browser test reads the rendered `<dl>`.
- Removes: `regimeLine`, whose only caller this task deletes. Its six test cases in `config.test.ts`'s `describe('regimeLine')` migrate to `regimeFacts` — *watch state unknown when no probe*, *a probe that succeeded*, *why a probe failed*, *never scanned rather than a null*, *carries the last scan status*, *no interval set* — each asserting the same intent against the new shape.

- [ ] **Step 1: Write the failing test**

```ts
// web/src/config.test.ts — append
import { regimeFacts } from './config'

describe('regimeFacts', () => {
  const root = {
    id: 1,
    path: '/library',
    scan_interval_secs: null,
    last_scan_at: null,
    last_scan_status: null,
  } as Root

  it('names each fact it states', () => {
    expect(regimeFacts(root)).toEqual([
      { label: 'watch', value: 'unknown — probed when a root is added' },
      { label: 'scan every', value: 'not set' },
      { label: 'last scan', value: 'never' },
    ])
  })

  it('keeps the outcome with the date rather than beside the other facts', () => {
    const scanned = {
      ...root,
      last_scan_at: '2026-09-17 08:33:49',
      last_scan_status: 'ok',
    }
    expect(regimeFacts(scanned)[2]).toEqual({
      label: 'last scan',
      value: '2026-09-17 08:33:49 · ok',
    })
  })

  it('states an unrecorded outcome rather than omitting it', () => {
    const scanned = {
      ...root,
      last_scan_at: '2026-09-17 08:33:49',
      last_scan_status: null,
    }
    expect(regimeFacts(scanned)[2].value).toBe(
      '2026-09-17 08:33:49 · outcome unrecorded',
    )
  })

  it('reports a failed watch probe with its reason', () => {
    expect(
      regimeFacts(root, { watching: false, watch_reason: 'inotify limit' })[0],
    ).toEqual({ label: 'watch', value: 'probe failed: inotify limit' })
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project node src/config.test.ts --reporter=default
```
Expected: FAIL — `regimeFacts is not a function`.

- [ ] **Step 3: Write `regimeFacts`**

```ts
// web/src/config.ts — directly above regimeLine

/**
 * The same three facts `regimeLine` joins into a sentence, each paired with
 * what it is a fact *about*.
 *
 * The sentence form put four `·`-separated pieces on one line — three facts,
 * the third of which appends its own outcome — and a value alone does not say
 * what it is: `unknown — probed when a root is added` is only legible beside
 * the word `watch`, and `never` beside `last scan`. Nothing here is ever
 * absent; each arm has a fallback, which is why labels rather than omission
 * are what make a missing fact readable (spec §11.6).
 *
 * **`regimeLine` goes with the sentence it built.** Checked rather than
 * assumed: its only production caller was the `<p className="regime">` this
 * task replaces — `RootRow.tsx` has no `aria-label` anywhere — so keeping it
 * would leave a function alive on the strength of its own tests. Its six
 * cases move here, each keeping its intent.
 */
export function regimeFacts(
  root: Root,
  probe?: Probe,
): Array<{ label: string; value: string }> {
  const watch =
    probe === undefined
      ? 'unknown — probed when a root is added'
      : probe.watching
        ? 'watching'
        : probe.watch_reason === null
          ? 'probe failed'
          : `probe failed: ${probe.watch_reason}`

  const interval =
    root.scan_interval_secs === null
      ? 'not set'
      : humanInterval(root.scan_interval_secs)

  const last =
    root.last_scan_at === null
      ? 'never'
      : `${root.last_scan_at} · ${root.last_scan_status ?? 'outcome unrecorded'}`

  return [
    { label: 'watch', value: watch },
    { label: 'scan every', value: interval },
    { label: 'last scan', value: last },
  ]
}
```

- [ ] **Step 4: Run it and watch it pass**

```bash
cd web && npx vitest run --project node src/config.test.ts --reporter=default
```
Expected: PASS.

- [ ] **Step 5: Render the `<dl>`**

```tsx
// web/src/RootRow.tsx — replace line 439
<dl className="regime">
  {regimeFacts(root, probe).map(({ label, value }) => (
    <div key={label}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  ))}
</dl>
```

Add `regimeFacts` to the existing import from `./config` at `RootRow.tsx:70`. Leave `regimeLine` imported only if something still uses it; if nothing does, drop it from the import and leave the export in place.

- [ ] **Step 6: Style it**

```css
/* admin.css — replacing the deleted `.root-row .regime { opacity: 0.8 }` */

/* Three independent facts, each naming what it is a fact about. A sentence
   was the wrong shape for them: `unknown — probed when a root is added` does
   not say what is unknown (spec §11.6). */
.root-row .regime {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: var(--space-1) var(--space-3);
  margin: var(--space-2) 0 0;
  font-size: var(--text-meta);
}

.root-row .regime > div {
  display: contents;
}

.root-row .regime dt {
  color: var(--muted);
}

.root-row .regime dd {
  margin: 0;
  color: var(--text);
}
```

- [ ] **Step 7: Update the row test**

```tsx
// web/src/RootRow.test.tsx — replace whichever assertion reads the sentence
it('states each fact under its own name', () => {
  const host = render(<RootRow root={root} {...noopHandlers} />)
  const pairs = [...host.querySelectorAll('.regime > div')].map((row) => [
    row.querySelector('dt')?.textContent,
    row.querySelector('dd')?.textContent,
  ])
  // Asserted per pair, not with a whole-element `toContain`: a page-wide
  // string cannot see a label attached to the wrong value.
  expect(pairs).toEqual([
    ['watch', 'unknown — probed when a root is added'],
    ['scan every', 'not set'],
    ['last scan', 'never'],
  ])
})
```

Run the existing file first and fix whatever the markup change broke:

```bash
cd web && npx vitest run --project jsdom src/RootRow.test.tsx --reporter=default
```

- [ ] **Step 8: Watch the new tests fail against their own defects**

1. Swap the order of the `scan every` and `last scan` entries in `regimeFacts` → the pairing test fails. (An ordering test that only moves a step *later* passes against the wrong thing — move it earlier too.)
2. Change `'not set'` to `''` → the first `config.test.ts` case fails.
3. Drop `display: contents` from `.regime > div` — this one is **not** caught by any test here, and that is expected: it is geometry, and Task 7's browser test is where it is measured.

Restore after each.

- [ ] **Step 9: Commit**

```bash
git add web/src/config.ts web/src/config.test.ts web/src/RootRow.tsx web/src/RootRow.test.tsx web/src/styles/admin.css
git commit -m "⑤b: a root states its three facts under their own names"
```

---

## Task 5: A zero is quieter than a number

The report draws **47** `data-field` elements — the number `ScanReport.test.tsx` already asserts — and on a real scan of this library **36** of them read `0` or `no`. CSS cannot select on text content, so the component marks the row (spec §11.8).

**Files:**
- Modify: `web/src/ScanReport.tsx` (`Count`, `Paths`, the `refused` row)
- Modify: `web/src/styles/admin.css`
- Test: `web/src/ScanReport.test.tsx`

**Interfaces:**
- Consumes: nothing.
- Produces: the attribute `data-zero` on a `.count`/`.paths` wrapper whose value is the nothing-happened value. Task 7 reads it in the browser.

- [ ] **Step 1: Write the failing test**

```tsx
// web/src/ScanReport.test.tsx — append

it('marks a zero row and leaves a row with a value unmarked', () => {
  const host = render(<ScanReport report={reportWithOneUpdate} />)
  const zero = host.querySelector('[data-field="added"]')
  const value = host.querySelector('[data-field="updated"]')
  // The fixture needs both, or this proves only that the attribute can be
  // written. `added` is 0 and `updated` is 1 in `reportWithOneUpdate`.
  expect(zero?.getAttribute('data-zero')).toBe('')
  expect(value?.hasAttribute('data-zero')).toBe(false)
})

it('marks an empty path list and leaves a populated one unmarked', () => {
  const host = render(<ScanReport report={reportWithOneUnreadable} />)
  expect(
    host.querySelector('[data-field="unreadable"]')?.hasAttribute('data-zero'),
  ).toBe(false)
  expect(
    host.querySelector('[data-field="gone"]')?.getAttribute('data-zero'),
  ).toBe('')
})

it('treats a scan that was not refused as a zero row', () => {
  const host = render(<ScanReport report={reportWithOneUpdate} />)
  expect(
    host.querySelector('[data-field="refused"]')?.getAttribute('data-zero'),
  ).toBe('')
})
```

Build `reportWithOneUpdate` and `reportWithOneUnreadable` from whatever fixture factory the file already has — do not hand-write a whole report object if one exists. Both fixtures must contain a zero row *and* a non-zero row.

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project jsdom src/ScanReport.test.tsx --reporter=default
```
Expected: FAIL on all three — no `data-zero` attribute exists.

- [ ] **Step 3: Mark the rows**

```tsx
// web/src/ScanReport.tsx — Count
function Count({
  field,
  label,
  n,
}: {
  field: string
  label: string
  n: number
}) {
  return (
    <div className="count" data-field={field} data-zero={n === 0 ? '' : undefined}>
      <dt>{label}</dt>
      <dd>{n}</dd>
    </div>
  )
}
```

```tsx
// web/src/ScanReport.tsx — Paths
<div
  className="paths"
  data-field={field}
  data-zero={paths.length === 0 ? '' : undefined}
>
```

```tsx
// web/src/ScanReport.tsx — the refused row, around line 236
<div className="count" data-field="refused" data-zero={walk.refused === null ? '' : undefined}>
  <dt>Refused</dt>
  <dd>{walk.refused === null ? 'no' : 'yes'}</dd>
</div>
```

`data-zero={cond ? '' : undefined}` is the idiom that produces a bare attribute or none at all; `data-zero={false}` renders the string `"false"`.

- [ ] **Step 4: Style it**

```css
/* admin.css — in the scan-report section */

/* 36 of this report's 47 facts read `0` or `no` on a clean scan. They are
   not noise — "nothing was refused" is an answer somebody came for, and O90
   closed by making this report *name* what it skipped — so none is hidden.
   A zero simply recedes to the weight of its own label (spec §11.7). */
.scan-report .count dd,
.scan-report .paths dd {
  color: var(--text);
  font-family: var(--font-figures);
  font-variant-numeric: tabular-nums;
}

.scan-report [data-zero] dd {
  color: var(--muted);
}
```

- [ ] **Step 5: Run the tests**

```bash
cd web && npx vitest run --project jsdom src/ScanReport.test.tsx --reporter=default
```
Expected: PASS, including the existing assertion that the report draws 47 `data-field` elements.

- [ ] **Step 6: Watch each test fail against its own defect**

1. Change `n === 0` to `n >= 0` → the first test fails on the `updated` row. (Pinning it forced-true *and* forced-false is not enough on its own; a fixture whose rows cancel out can hide a flipped comparison — this is why both fixtures carry a zero and a non-zero row.)
2. Change `paths.length === 0` to `false` → the second fails.
3. Change `walk.refused === null` to `walk.refused !== null` → the third fails.

Restore after each.

- [ ] **Step 7: Commit**

```bash
git add web/src/ScanReport.tsx web/src/ScanReport.test.tsx web/src/styles/admin.css
git commit -m "⑤b: a zero in the scan report recedes without being hidden"
```

---

## Task 6: `--danger` reaches the two removal blocks

`--danger` is defined in `tokens.css:54`, its contrast is computed on three grounds in both themes by `index.css.test.ts`'s `TEXT_TOKENS` floors, and **it is referenced by no rule in any stylesheet** — verified at `149a5b49`.

The real copy in these blocks is not what the ⑤b mockup drew. Read it before styling it: the root block's button is `Remove ${root.path}`, and its irreversibility line is a full paragraph ("Nothing here can be undone from this screen. The files on disk are untouched; …"), not "This cannot be undone". Both blocks have `.counts` and `.irreversible`; the root block also has a `<details className="removal-derive">` and a Cancel button.

**Files:**
- Modify: `web/src/styles/admin.css` (`.remove-root` at 1188, `.remove-user` at 1330)
- Modify: `web/src/RemoveRoot.tsx:145`, `web/src/UserRow.tsx:626` (one `className="confirm"` each)
- Test: `web/src/styles/admin-config.css.test.ts` (the `color-mix` case from Task 3 turns green here)

**Interfaces:**
- Consumes: `--danger` from `tokens.css`.
- Produces: nothing other tasks read.

- [ ] **Step 1: Rewrite both blocks**

```css
/* The most destructive action in the product. C4 §2 opened by saying it did
   not look like one, and C4's own answer was the counts this block still
   renders — the live figure, stated before it acts. This is the other half:
   the `--danger` token on the edge and on the verb.

   Not a filled button. The block only appears after "Remove…", it states the
   real cost, and it says what cannot be undone — the step is already
   guarded, and the loudest control on the screen would be shouting at
   somebody who has committed to it (spec §11.5).

   The inset strip gains a third meaning here; it already marks a strong
   proposal and a selected rule. In all three it says *this row is not like
   the others*, which is the same claim. */
.remove-root,
.remove-user {
  border: 1px solid var(--danger);
  box-shadow: inset 3px 0 0 var(--danger);
  border-radius: var(--radius-control);
  margin-top: var(--space-4);
  padding: var(--space-4);
}

.remove-root .counts,
.remove-user .counts {
  font-weight: 600;
}

.remove-root .irreversible,
.remove-user .irreversible {
  color: var(--muted);
  font-size: var(--text-meta);
  max-width: 60ch;
}

/* The button that does the thing takes the token on its border and its
   text; everything else in the block keeps the ordinary control look. */
.remove-root .confirm,
.remove-user .confirm {
  border-color: var(--danger);
  color: var(--danger);
  font-weight: 600;
}
```

**A class, not `:first-of-type`.** The obvious rule here is
`.remove-root > button:first-of-type`, and it is wrong: both blocks render
their confirm button only once the cost has loaded
(`{cost !== undefined && …}`), and until then their only button is
**Cancel** — which `:first-of-type` would then draw in `--danger`, on the one
screen state where nothing destructive is even offered yet. Checked in the
markup rather than reasoned about: `RemoveRoot.tsx:145` and
`UserRow.tsx:626` are the confirm buttons, each inside a fragment that a
`cost === undefined` render omits entirely, and each followed by a Cancel
button that is always present.

`UserRow`'s already carries `name="confirm-remove"` and its Cancel carries
`name="cancel-remove"`, so that pair could be styled without touching the
markup at all — but `RemoveRoot`'s two buttons have no `name`, and a `name`
is a form-submission concern rather than a styling hook. One class both
blocks share is the thing that makes the rule read the same in both.

So this task also adds `className="confirm"` to both confirm buttons:

```tsx
// web/src/RemoveRoot.tsx — the button reading `Remove ${root.path}`
<button type="button" className="confirm" onClick={confirm} disabled={removing}>
  {removing ? 'Removing…' : `Remove ${root.path}`}
</button>
```

```tsx
// web/src/UserRow.tsx — the button reading `Remove ${shown.name}`
// same change: add className="confirm", touch nothing else
```

`.remove-user` had no `margin-top` and now gets one from the shared rule. Confirm in the browser that this does not add a gap where the layout did not have one; if it does, keep the two `margin-top` values separate.

- [ ] **Step 2: Add the assertion that closes the sweep**

```ts
// web/src/styles/admin-config.css.test.ts — append to the same describe
it('mixes no border from currentColor', () => {
  const found = [
    ...withoutComments(adminSource).matchAll(
      /color-mix\(in srgb, currentColor[^)]*\)/g,
    ),
  ].map((m) => m[0])
  expect(found).toEqual([])
})
```

Write it **before** the CSS change in this task if you want to watch it fail;
either way it must be green in the same commit that removes the last two.

- [ ] **Step 3: Run the sweep test**

```bash
cd web && npx vitest run --project node src/styles/admin-config.css.test.ts --reporter=default
```
Expected: both cases PASS — the last two `color-mix` uses are gone.

- [ ] **Step 4: Check the contrast floors still hold**

```bash
cd web && npx vitest run --project node src/index.css.test.ts --reporter=default
```
Expected: PASS. `--danger` is already in `TEXT_TOKENS`, so its 4.5:1 on `--bg`, `--surface` and `--field` in both themes is already computed — this task adds no new floor, it makes an existing one load-bearing.

- [ ] **Step 5: Pin the state that the obvious rule got wrong**

```tsx
// web/src/RemoveRoot.test.tsx — append
it('marks the confirm button and not Cancel, before the cost has loaded', () => {
  // The whole reason this is a class and not `:first-of-type`: until the
  // cost resolves, Cancel is the only button in the block.
  const host = render(<RemoveRoot root={root} {...noopHandlers} />)
  expect(host.querySelectorAll('.remove-root button')).toHaveLength(1)
  expect(host.querySelector('.remove-root .confirm')).toBeNull()
})
```

Run it, then resolve the cost and assert the confirm button carries `.confirm`
and the Cancel button does not. Follow whatever the file already does to
resolve a pending fetch — do not invent a second way.

- [ ] **Step 6: Look at it**

Open `/config/roots` and `/config/users` in both themes, press "Remove…", and crop the block — both while it reads "Working out what this would remove…" and after. The measurement can be right while the noun is wrong; confirm the strip is on the block and the colour is on the button you would actually press, not on Cancel.

- [ ] **Step 7: Commit**

```bash
git add web/src/styles/admin.css web/src/styles/admin-config.css.test.ts web/src/RemoveRoot.tsx web/src/UserRow.tsx web/src/RemoveRoot.test.tsx
git commit -m "⑤b: --danger reaches the two blocks that remove things

Defined since the tokens landed, contrast-verified in both themes by
index.css.test.ts, and until now referenced by no rule in any stylesheet.

The confirm button carries a class rather than :first-of-type: both blocks
render their confirm button only once the cost has loaded, so until then
their only button is Cancel — which :first-of-type would have drawn in
--danger on the one state where nothing destructive is offered yet."
```

---

## Task 7: The browser pass

Nothing above is measurable in jsdom. ⑤a shipped two defects into exactly this gap, and its declaration tests could not see an override lower in the same file.

**Files:**
- Test: `web/src/Config.browser.test.tsx` (**new**, `browser` project)
- Test: `web/src/styles/admin-config.css.test.ts` (extended)

**Interfaces:**
- Consumes: `.field`, `.checks` (Task 1), `--config-measure` (Task 2), `.regime > div` (Task 4), `data-zero` (Task 5).
- Produces: nothing.

- [ ] **Step 1: Write the failing browser test**

```tsx
// web/src/Config.browser.test.tsx — new file
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import './index.css'
import * as config from './config'
import { Roots } from './Roots'
import { Users } from './Users'
import { mount } from './testing/mount'

// The three screens take no props and fetch their own data, so this file
// stubs `./config` exactly as `Roots.test.tsx:7` and `Users.test.tsx` do.
// `mount` keeps ONE module-level host, torn down in afterEach — one mount
// per `it()`, never two, or the second's host leaks into document.body.
vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
  listUsers: vi.fn(),
  // …the rest of the surface each screen calls; copy the list from the
  // matching jsdom test rather than guessing which ones are needed.
}))

describe('the Configuration screens at 390px', () => {
  it('does not scroll sideways on the roots screen', async () => {
    await page.viewport(390, 844)
    vi.mocked(config.listRoots).mockResolvedValue([longPathRoot])
    const host = await act(async () => mount(<Roots />))
    const doc = document.documentElement
    const main = host.closest('main') ?? document.querySelector('main')
    // Both, not just the document: ⑤a's assertions read only the document
    // and two defects shipped through `main` underneath it.
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
    expect(main!.scrollWidth).toBeLessThanOrEqual(main!.clientWidth)
  })

  // …the same for <Users /> and <Backup />, one mount per test.

  it('keeps every control inside the card that holds it', async () => {
    await page.viewport(1280, 900)
    vi.mocked(config.listUsers).mockResolvedValue([user])
    const host = await act(async () => mount(<Users />))
    const card = host.querySelector('.user-row') as HTMLElement
    const cardBox = card.getBoundingClientRect()
    for (const control of card.querySelectorAll('input, select, textarea')) {
      expect(control.getBoundingClientRect().width).toBeLessThanOrEqual(
        cardBox.width,
      )
    }
    // And the card itself is bounded, which is the rule that makes the
    // above hold for a control that has no width of its own.
    expect(cardBox.width).toBeLessThanOrEqual(40 * 16)
  })

  it('puts each root fact on its own row beside its label', async () => {
    await page.viewport(1280, 900)
    vi.mocked(config.listRoots).mockResolvedValue([longPathRoot])
    const host = await act(async () => mount(<Roots />))
    const rows = [...host.querySelectorAll('.regime > div')]
    expect(rows).toHaveLength(3)
    const tops = rows.map(
      (row) => row.querySelector('dt')!.getBoundingClientRect().top,
    )
    // Three distinct rows: `display: contents` is what makes the grid place
    // the dt/dd pairs, and dropping it collapses them onto one line.
    expect(new Set(tops).size).toBe(3)
    for (const row of rows) {
      const dt = row.querySelector('dt')!.getBoundingClientRect()
      const dd = row.querySelector('dd')!.getBoundingClientRect()
      expect(dd.left).toBeGreaterThan(dt.right - 1)
    }
  })

  it('draws a zero more quietly than a value', async () => {
    await page.viewport(1280, 900)
    // The scan report needs a report with a zero row AND a non-zero row;
    // reuse the fixture `ScanReport.test.tsx` already builds.
    const host = await act(async () => mount(<ScanReport report={mixed} />))
    const zero = host.querySelector('[data-zero] dd') as HTMLElement
    const value = host.querySelector('.count:not([data-zero]) dd') as HTMLElement
    expect(zero).not.toBeNull()
    expect(value).not.toBeNull()
    expect(getComputedStyle(zero).color).not.toBe(getComputedStyle(value).color)
  })
})
```

`longPathRoot` must carry a **real** path from this library, not a short
fixture one. ⑤a's C1 hid behind a ten-character `/srv/manga`: the defect
needed a 35-character root to appear, and the test that should have caught
it passed. Use `/home/davey/temp/manga-test/Volumes`.

`page.viewport()` is the only thing that changes `clientWidth`; setting
`documentElement.style.width` does not.

The `for (const screen of [...])` loop the first draft of this plan used is
**not** available: these screens take no props and are not reached by a
route here, so each gets its own `it()` with its own mock and its own single
mount.

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project browser src/Config.browser.test.tsx --reporter=default
```
Expected: FAIL until the mount helper is right. Get it failing for the *right* reason — an assertion, not a missing import — before implementing anything.

- [ ] **Step 3: Make them pass**

The rules from Tasks 1–6 should already satisfy these. Where one does not, fix the CSS, not the assertion.

- [ ] **Step 4: Watch each test fail against its own defect**

1. Remove `max-width: var(--config-measure)` from `.user-row` → the measure test fails.
2. Remove `display: contents` from `.root-row .regime > div` → the fact-row test fails. (This is the assertion Task 4 Step 8 deliberately left to here.)
3. Remove `.scan-report [data-zero] dd { color: var(--muted) }` → the quietness test fails.
4. Set `.root-row { min-width: 60rem }` → the 390 px tests fail on `main`, and confirm they fail on `main` rather than only on the document.

Restore after each. Run with `--reporter=default` and put a control `console.error` in one test first — a zero read from a silenced reporter is not a zero.

- [ ] **Step 5: Extend the CSS test with the measure**

```ts
// web/src/styles/admin-config.css.test.ts — append
it('sets the measure once rather than per control', () => {
  const widths = rules(adminSource)
    .filter(({ prelude }) => /\.(root-row|user-row|add-user|add-root)\b/.test(prelude))
    .filter(({ body }) => /(?:^|;)\s*width:/.test(body))
    .map(({ prelude }) => prelude)
  // A `width` on a control inside these cards is the drift §11.4 refuses:
  // a figure per control is a figure to re-measure per review round.
  expect(widths).toEqual([])
})
```

- [ ] **Step 6: Commit**

```bash
git add web/src/Config.browser.test.tsx web/src/styles/admin-config.css.test.ts
git commit -m "⑤b: the browser pass over all four Configuration screens"
```

---

## Task 8: The floor gate learns the two shapes it is blind to

**Do this task only after this branch is rebased onto a `main` that contains #173.** See "The one ordering constraint" above.

`base.css.test.ts`'s own doc comment already names what it cannot see. Two of those are live in one rule in this tree, and O151 is filed because of them:

- **The `font` shorthand** — the match is on `font-size:` alone, so `font: inherit` passes unseen.
- **A subject wrapped in `:where()`/`:is()`** — `subject()` returns `:where(input, select)`, which `CONTROL_SUBJECT` does not match.

**An attribute-only subject** (`.field [type="text"]`) is the third limit in that comment and is **not** widened here: the gate would have to know which element an attribute selector matches, which it cannot. Leave that paragraph in the doc comment exactly as it stands.

**Files:**
- Modify: `web/src/styles/base.css.test.ts`
- Modify: `web/src/styles/admin.css` (one companion for `.account input`)

**Interfaces:**
- Consumes: `rules`, `selectors`, `subject`, `withoutComments` from `../testing/css` — all already imported.
- Produces: nothing.

- [ ] **Step 1: Write the failing test**

```ts
// web/src/styles/base.css.test.ts — append inside the floor describe

it('sees a control whose size comes from the font shorthand', () => {
  // `font: inherit` sets font-size, and the gate matched `font-size:` alone.
  // This is half of O151: the reader's Settings panel hands `--text-ui` to
  // every control through exactly this declaration.
  const exposed = fontSizedControls('.panel input { font: inherit; }')
  expect(exposed).toEqual([
    { selector: '.panel input', value: 'font: inherit' },
  ])
  expect(needsAFloor('font: inherit')).toBe(true)
})

it('sees a subject wrapped in :where() or :is()', () => {
  // The other half of O151. `:where()` carries no specificity, which is no
  // defence at all: the floor is (0,0,0), so one class beats it.
  expect(
    fontSizedControls('.panel :where(input, select) { font-size: 14px; }'),
  ).toEqual([
    { selector: '.panel :where(input)', value: '14px' },
    { selector: '.panel :where(select)', value: '14px' },
  ])
})

it('pairs a companion that covers more controls than the rule it corrects', () => {
  // #173's companion names `textarea` although the panel draws none, so its
  // selector text differs from the exposed rule by one name. Expanding both
  // is what makes the superset pair; exact-text matching would not.
  const exposed = fontSizedControls('.p :where(input, select) { font: inherit; }')
  const floored = new Set(
    fontSizedControls(
      '.p :where(input, select, textarea) { font-size: max(16px, 1rem); }',
    ).map((c) => c.selector),
  )
  expect(exposed.every(({ selector }) => floored.has(selector))).toBe(true)
})

it('does not expand a wrapped list that is not all controls', () => {
  // `:is(input, .foo)` takes its most specific argument's specificity, so
  // narrowing it to `:is(input)` would change what it beats. Left whole, it
  // is dropped instead. Dropped is not strict: nothing can go red for it.
  expect(
    fontSizedControls('.p :is(input, .foo) { font-size: 14px; }'),
  ).toEqual([])
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project node src/styles/base.css.test.ts --reporter=default
```
Expected: the first three FAIL, the fourth passes vacuously (nothing is expanded yet).

- [ ] **Step 3: Widen `fontSizedControls`**

```ts
/**
 * Every selector in `selector`'s place that styles exactly one control,
 * with its wrapper preserved.
 *
 * A wrapped subject — `.panel :where(input, select)` — is narrowed to one
 * control at a time *inside the same wrapper*, never unwrapped: `:where()`
 * contributes zero and `:is()` contributes its most specific argument, so
 * `:where(input)` and `:where(input, select)` have identical specificity and
 * the "two rules sharing one selector always tie" guarantee below survives
 * the expansion intact. Unwrapping to a bare `input` would not.
 *
 * **A list that is not all controls is left whole**, because `:is()` takes
 * its most specific argument: narrowing `:is(input, .foo)` from (0,1,0) to
 * (0,0,1) would silently change what it has to beat. Left whole it matches
 * nothing here and is dropped — which is not the same as strict, since a
 * dropped rule never enters `exposed`. Stated as a limit, not a defence.
 *
 * `:not()` and `:has()` are not expanded and must not be: `.foo:not(input)`
 * styles `.foo`, not an input.
 */
function expandControlSubjects(selector: string): string[] {
  const subj = subject(selector)
  const prefix = selector.slice(0, selector.length - subj.length)
  if (CONTROL_SUBJECT.test(subj)) return [selector]
  const wrapped = /^(:where|:is)\((.*)\)([^)]*)$/.exec(subj)
  if (!wrapped) return []
  const parts = selectors(wrapped[2])
  if (!parts.every((part) => CONTROL_SUBJECT.test(part))) return []
  return parts.map(
    (part) => `${prefix}${wrapped[1]}(${part})${wrapped[3]}`,
  )
}
```

Then in `fontSizedControls`, read the shorthand too and expand each selector:

```ts
function fontSizedControls(
  css: string,
): Array<{ selector: string; value: string }> {
  return rules(css).flatMap(({ prelude, body }) => {
    const sized = body.match(/(?:^|;)\s*font-size:\s*([^;]+)/)
    const shorthand = body.match(/(?:^|;)\s*font:\s*([^;]+)/)
    // `font-size` wins when a rule carries both: it comes after the
    // shorthand in every rule in this tree, and the shorthand's own size is
    // what it would override.
    const value = sized
      ? sized[1].trim()
      : shorthand
        ? `font: ${shorthand[1].trim()}`
        : null
    if (value === null) return []
    return selectors(prelude)
      .flatMap(expandControlSubjects)
      .map((selector) => ({ selector, value }))
  })
}
```

And teach `needsAFloor` the shorthand:

```ts
function needsAFloor(value: string): boolean {
  // A `font` shorthand is never proven. Its size sits between optional
  // style/variant/weight/stretch keywords and a mandatory family, and
  // `font: inherit` resolves against an ancestor this gate has no model of
  // — which is exactly how the reader's panel handed 14px to every control
  // it drew (O151). Unprovable is the same as under the line here.
  if (value.startsWith('font: ')) return !/max\(\s*16px/.test(value)
  if (/^max\(\s*16px/.test(value)) return false
  const rem = value.match(/^(-?\d*\.?\d+)rem$/)
  if (rem) return Number.parseFloat(rem[1]) * 16 < 16
  const px = value.match(/^(-?\d*\.?\d+)px$/)
  if (px) return Number.parseFloat(px[1]) < 16
  return true
}
```

- [ ] **Step 4: Run the whole file**

```bash
cd web && npx vitest run --project node src/styles/base.css.test.ts --reporter=default
```
Expected: the four new cases PASS, and the cascade case **FAILS** with exactly one uncovered selector, `.account input`. That figure was measured by simulating this gate against the post-#173 cascade before the plan was written — if you see three, the rebase has not happened and the other two are the reader's, which #173 fixes.

- [ ] **Step 5: Give `.account input` its companion**

```css
/* admin.css — immediately after the `.account input` rule at 1383 */

/* `font: inherit` cannot be proven to clear 16px from the text alone: it
   resolves against whatever `.account` inherits, and this rule outranks
   `base.css`'s zero-specificity floor at (0,1,1). It computes 16px today —
   `.account` sets no size — so this changes nothing on any device. It is
   here because the same shape, one screen over, is what made the reader's
   Settings panel zoom iOS on focus (O151), and the gate that now sees it
   should not have an exception written into it. */
@media (pointer: coarse) {
  .account input {
    font-size: max(16px, var(--text-ui));
  }
}
```

The companion must come **after** the rule it corrects — equal specificity is decided by source order, and the existing `places admin.css's companion after the rule it corrects` test asserts exactly that for `.field input`.

- [ ] **Step 6: Run it green**

```bash
cd web && npx vitest run --project node src/styles/base.css.test.ts --reporter=default
```
Expected: PASS, all cases.

- [ ] **Step 7: Watch the widened gate fail against the defect it exists for**

1. Put `font: inherit` on a control without a companion — add `.root-row .field input { font: inherit }` to `admin.css` → the cascade test fails naming it. Restore.
2. Change the new companion's value from `max(16px, var(--text-ui))` to `var(--text-ui)` → the cascade test fails again, because `floored` is filtered by value and not merely by selector. Restore.
3. Revert `expandControlSubjects` to return `[]` for a wrapped subject → the wrapped-subject test fails. Restore.

- [ ] **Step 8: Update the doc comment**

Delete the two paragraphs in `fontSizedControls`'s comment that say the gate cannot see the `font` shorthand and a wrapped subject — they are no longer true. **Keep** the attribute-only paragraph and the closing paragraph about the reader, rewriting the latter to say the gate now sees it and #173 fixed it. An understated limit is worse than none, and so is a stale one.

- [ ] **Step 9: Commit**

```bash
git add web/src/styles/base.css.test.ts web/src/styles/admin.css
git commit -m "⑤b: the 16px floor gate learns the font shorthand and a wrapped subject

Both are live in one rule — reader.css's `.reader-dialog-panel
:where(input, select) { font: inherit }` — which is why a device found O151
and this gate did not. Its first catch on the way in is `.account input`,
which computes 16px today and is unprovable from the text."
```

---

## Task 9: The gate, the device pass and the claim check

- [ ] **Step 1: Run the full web gate**

```bash
# LOG goes in this session's scratchpad directory, not the repo and not $HOME.
LOG=<your scratchpad>/gate.log
cd /home/davey/projects/taiyaku && NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 \
  scripts/check-all.sh >| "$LOG" 2>&1; echo "EXIT=$?" >> "$LOG"
grep -c "all green" "$LOG"; tail -3 "$LOG"
```

Read `EXIT=` from the file. Do not pipe the gate through `tail` and do not append `; echo` to it — both report the pipeline's status, and this has already reported a green over a red run once.

- [ ] **Step 2: Look at the pixels**

Open all four screens at 390 px and at desk width, in both themes. Crop the regions before writing anything up — the measurement keeps being right while the noun is wrong.

- [ ] **Step 3: The device pass**

Only a real iOS device can show the `pointer: coarse` floor working; driven Chromium is `pointer: fine` and the media query never fires. Serve the production build over the tailnet and check that tapping a field on `/config/users` does not zoom the document.

- [ ] **Step 4: Update `docs/status.md`**

Close nothing that this PR did not close. O151's row is closed by #173, not here; this PR's gate widening is the second half of its "what would settle it" and the row should say so.

- [ ] **Step 5: The claim check, as its own pass**

Check every factual claim in the PR body, the status rows and each commit message — separately, after the gate is green, and again after the fix round. On the last four PRs this found wrong sentences *after* CI was green, and the later passes kept finding defects inside the earlier passes' own fixes. Prefer deleting a figure to correcting it a fourth time.

---

## Self-Review

**Spec coverage** — every section of §11 maps to a task:

| spec | task |
|---|---|
| §11.3 three kinds, three treatments | 1 |
| §11.4 the measure | 2, 7 |
| §11.5 the destructive blocks | 6 |
| §11.6 a root's state | 4 |
| §11.7 the scan report | 5 |
| §11.8 the three markup changes | 1 (checkboxes/prose), 4 (`<dl>`), 5 (`data-zero`) |
| §11.9 the floor gate | 8 |
| §11.10 not in this PR | — (O148/O149/O150 untouched) |
| §11.11 tests and checks | 3 (sweep), 7 (measure, zero marking, no sideways scroll), 6 (`--danger`), every task's last-but-one step (watched failing) |
| §11.1 the five `color-mix` and nine `opacity` | 3, 6 |

**Five things the spec did not name, found while reading the code, all folded in rather than filed:**
- `.user-row .fields label` has no rule, so the Role label is inline flow — the same defect as the password field (Task 1).
- `.fields` is written twice, differing only in a `margin-top`, so every fix lands in both (Task 1). It was called byte-identical here and in the spec, and never was — `.user-row .fields` has carried that margin since before the branch.
- **`.field` has three selector lists, not one.** Its `font-size: 0.7rem`, `letter-spacing` and `text-transform: uppercase` are inherited properties that reach the control; `admin.css:529` is the reset and `admin.css:566` the coarse-pointer companion. Adding a host to the first alone renders every control at 11.2 px uppercase *and* puts it under the iOS floor (Task 1). `textarea` is new to the last two lists, for the root row's `exclude_globs` box.
- **`.roots .add-root label` and `.users .add-user label` outrank `.field`** at (0,2,1) to (0,2,0), so `.field`'s `display: grid` would lose while its inherited uppercase still applied. Both are deleted (Task 1).
- **`button:first-of-type` is the wrong hook for the confirm button.** Both removal blocks render it only once the cost has loaded, so until then their only button is Cancel — which that selector would draw in `--danger` on the one state where nothing destructive is offered. A class instead, and a test pinning the loading state (Task 6).

**Type consistency** — `regimeFacts` returns `Array<{ label: string; value: string }>` in Task 4 and is read as `{ label, value }` in Task 4's own render and test; `data-zero` is written in Task 5 and read in Tasks 5 and 7; `--config-measure` is defined in Task 2 and read in Tasks 2 and 7; `expandControlSubjects` is defined and called in Task 8 only.

**Ordering** — Tasks 1–7 are independent of #173. Task 8 requires the rebase. Task 3 deliberately leaves two `color-mix` uses for Task 6 and says so in its own commit message rather than weakening its test.

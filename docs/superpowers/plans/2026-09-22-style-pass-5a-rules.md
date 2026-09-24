# Style pass ⑤a — the Rules screen: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw the settings area's Rules screen — the one screen in the app that has never been styled — and stop it scrolling sideways at 390 px.

**Architecture:** All new rules go in a new section of `web/src/styles/admin.css`, placed immediately after the `.config-nav` block so it sits with the settings area rather than with the proposals inbox above it. One JSX move (`PreviewTabs` out of `.editor-right`), one shared-class hoist (`.proposal-detail .field` → `.field`), and two markup defect fixes. Nothing else changes behaviour.

**Tech Stack:** React 19, TypeScript, plain CSS with custom properties, Vitest across three projects — `node` (`src/**/*.test.ts`), `jsdom` (`src/**/*.test.tsx`, excluding `*.browser.test.tsx`), `browser` (`src/**/*.browser.test.tsx`, real Chromium).

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-09-17-style-pass-design.md` §10. §5's six-item checklist applies to this PR as to every other in the pass.
- **Colour comes from `styles/tokens.css` only.** §4 rule 2. `colourLiterals.test.ts` fails on any hex, `rgb()`/`rgba()`/`hsl()`/`hsla()` or CSS system-colour keyword added under `src/`, and its allowance only shrinks — do not add an entry.
- **No `opacity` for quiet text.** `admin.css`'s own `.proposal-detail` comment: quiet is bought with size and border. The one exemption is `--muted` on a disabled control.
- **Spacing, type and radius come from tokens**: `--space-1`…`--space-7`, `--text-h1`/`h2`/`h3`/`body`/`ui`/`meta`, `--radius-control`. `admin.css` uses none of them today; every rule this plan adds uses them.
- **Run the web gate with the thread throttle**: `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh`. An unthrottled run has been killed for memory on this machine.
- **Vitest reporter**: under Claude Code vitest picks an `agent` reporter that hides passing tests' console output. Use `--reporter=default` when you need to see it.
- **No AI/assistant attribution** in any commit message, comment or doc.

---

## A decision this plan changes, and why

§10.4 promises a second line on each rule row reading, in the mockup you approved, `regex on path → series`. **The `→ series` half cannot be honestly derived from the rule alone.** On this library, rule 1 is `kind: 'regex'`, `pattern: '^(?<series>[^/]+)/'`, `set_series: null` — it sets series through a *named capture group*, and the only way the client could say so is by parsing the pattern and reimplementing what the server's derive does. `Row.decided_by` is the server's own answer to "which rule decided which field", and it exists only once a preview has landed, so a line built from it would appear and disappear.

Putting a second opinion about derivation in the client is the shape O93 closed.

**This plan therefore builds the half that is definitive** — `kind` and `match_on`, plus `→ field` only where the rule states it outright (a non-null `set_*`, or a `builtin-volume`/`builtin-chapter` kind). On this library that gives:

| rule | second line |
|---|---|
| 1 `regex` on `path`, capture group | `regex on path` |
| 2 `builtin-volume` on `name` | `builtin on name → volume` |
| 3 `builtin-chapter` on `name` | `builtin on name → chapter` |
| 4 `regex` on `name`, capture group | `regex on name` |

If you would rather have the capture-group names too, it is one added clause in `ruleMechanism` (Task 2) and a note in the spec saying the client parses patterns for display. Raise it before Task 2.

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `web/src/rules.ts` | Adds `ruleName` and `ruleMechanism` — pure functions over one `Rule`, so they are testable in the `node` project without React | 2 |
| `web/src/RuleList.tsx` | Row markup: the separator, the two-line name, the ordinal | 1, 2, 4 |
| `web/src/PreviewTabs.tsx` | Tab badge separator; the scroll box around each table | 1, 7 |
| `web/src/RuleEditor.tsx` | Moves `<PreviewTabs>` out of `.editor-right` | 3 |
| `web/src/styles/admin.css` | Every new rule, in one new section after `.config-nav`; the `.field` hoist | 3, 4, 5, 6, 7, 8 |
| `web/src/index.css.test.ts` | Widened to read `base.css` for the figures-face check | 8 |
| `web/src/styles/admin-rules.css.test.ts` | **New**, `node` project: pins the CSS facts that are defects if they regress — `list-style`, the scroll box, the sticky cell's background, the type face | 4, 7, 8 |
| `web/src/PreviewTabs.browser.test.tsx` | **New**, `browser` project: real layout — the sticky column and the no-sideways-scroll assertion | 7 |

---

### Task 1: The two run-together strings

`4 rules1 override` and `Overrides1` are missing separators in the markup. A whole-element `toContain` cannot see a missing space, so both assertions are on exact text content.

**Files:**
- Modify: `web/src/RuleList.tsx:231-239`
- Modify: `web/src/PreviewTabs.tsx:134-147`
- Test: `web/src/RuleList.test.tsx`, `web/src/PreviewTabs.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: nothing later tasks rely on.

- [ ] **Step 1: Write the failing test in `web/src/RuleList.test.tsx`**

Add inside the existing `describe('RuleList', …)`:

```tsx
  it('separates the rule count from the override badge', () => {
    const { host } = render({ overrideCount: 1 })
    const count = host.querySelector('.rule-count')
    // Exact text, not `toContain`: the defect this pins is a *missing*
    // separator, and `toContain('1 override')` passes against `4 rules1
    // override` just as happily as against the correct string.
    expect(count?.textContent).toBe('2 rules · 1 override')
  })

  it('pluralises the override badge and keeps the separator', () => {
    const { host } = render({ overrideCount: 3 })
    expect(host.querySelector('.rule-count')?.textContent).toBe(
      '2 rules · 3 overrides',
    )
  })

  it('says nothing about overrides when there are none', () => {
    const { host } = render({ overrideCount: 0 })
    expect(host.querySelector('.rule-count')?.textContent).toBe('2 rules')
  })
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project jsdom src/RuleList.test.tsx -t 'separates the rule count'`
Expected: FAIL — `expected '2 rules1 override' to be '2 rules · 1 override'`

- [ ] **Step 3: Fix `web/src/RuleList.tsx`**

Replace the `.rule-count` paragraph:

```tsx
      <p className="rule-count">
        {rules.length} rules
        {overrideCount > 0 && (
          <>
            {' · '}
            <span className="override-badge">
              {overrideCount === 1
                ? '1 override'
                : `${overrideCount} overrides`}
            </span>
          </>
        )}
      </p>
```

- [ ] **Step 4: Run it and watch it pass**

Run: `cd web && npx vitest run --project jsdom src/RuleList.test.tsx`
Expected: PASS, whole file green.

- [ ] **Step 5: Write the failing test in `web/src/PreviewTabs.test.tsx`**

Add inside the top-level `describe`:

```tsx
  it('separates the Overrides tab label from its badge', () => {
    const { host } = render({ preview: preview({ overrides: [anOverride()] }) })
    const tab = [...host.querySelectorAll('.preview-tabs button')].find((b) =>
      b.textContent?.startsWith('Overrides'),
    )
    // Exact text again: `Overrides1` contains 'Overrides'.
    expect(tab?.textContent).toBe('Overrides 1')
  })
```

`preview()` and `anOverride()` are this file's existing factories, defined above its `render()` helper. Add no new ones.

- [ ] **Step 6: Run it and watch it fail**

Run: `cd web && npx vitest run --project jsdom src/PreviewTabs.test.tsx -t 'separates the Overrides tab'`
Expected: FAIL — `expected 'Overrides1' to be 'Overrides 1'`

- [ ] **Step 7: Fix `web/src/PreviewTabs.tsx`**

Inside the `TABS.map` button:

```tsx
            {name}
            {name === 'Overrides' && overrides.length > 0 && (
              <>
                {' '}
                <span className="badge">{overrides.length}</span>
              </>
            )}
```

- [ ] **Step 8: Run both files and watch them pass**

Run: `cd web && npx vitest run --project jsdom src/PreviewTabs.test.tsx src/RuleList.test.tsx`
Expected: PASS, both files green.

- [ ] **Step 9: Commit**

```bash
git add web/src/RuleList.tsx web/src/RuleList.test.tsx web/src/PreviewTabs.tsx web/src/PreviewTabs.test.tsx
git commit -m "Rules: separate the override badge from the text beside it

\`4 rules1 override\` and \`Overrides1\` were missing separators in the
markup rather than styling. Both assertions are on exact text content: a
\`toContain\` cannot see a missing space."
```

---

### Task 2: A rule's name, and the line beneath it

The label chain never consults `note`, so a `builtin-volume` or `builtin-chapter` rule — empty pattern, setting nothing — reads `new rule` permanently. Two of this library's four rules do.

**Files:**
- Modify: `web/src/rules.ts` (append after `blankRule`)
- Modify: `web/src/RuleList.tsx:167-183`
- Test: `web/src/rules.test.ts`, `web/src/RuleList.test.tsx`

**Interfaces:**
- Consumes: `Rule` from `./rules` (already exported).
- Produces: `ruleName(rule: Rule): string` and `ruleMechanism(rule: Rule): string`, both exported from `web/src/rules.ts`. Task 4 styles the two elements they fill.

- [ ] **Step 1: Write the failing tests in `web/src/rules.test.ts`**

Add at the end of the file. **Define `aRule` exactly as below — do NOT substitute `rules.test.ts`'s existing `rule()` factory.** That one sets `set_edition_key: 'raw'`, so the chain would return `'raw'` and two of the `ruleName` cases below would fail for a reason that has nothing to do with the code under test. `aRule` spreads `blankRule(1)`, whose every optional field is `null`, which is what these cases need:

```ts
import { ruleMechanism, ruleName } from './rules'

const aRule = (over: Partial<Rule> = {}): Rule => ({
  ...blankRule(1),
  ...over,
})

describe('ruleName', () => {
  it('prefers the note, which is the only thing a builtin has', () => {
    expect(
      ruleName(
        aRule({
          kind: 'builtin-volume',
          match_on: 'name',
          pattern: '',
          note: 'volume from the name',
        }),
      ),
    ).toBe('volume from the name')
  })

  it('falls past an empty note rather than stopping on it', () => {
    // `??` cannot see `''`, and the old chain's trailing `|| 'new rule'`
    // only caught the *last* item — so an empty note would have skipped
    // the pattern entirely and landed on 'new rule'.
    expect(ruleName(aRule({ note: '', pattern: 'Manga/.*' }))).toBe('Manga/.*')
  })

  it('falls past an empty note to a set field before the pattern', () => {
    expect(
      ruleName(aRule({ note: null, set_series: 'Akira', pattern: 'x' })),
    ).toBe('Akira')
  })

  it('says "new rule" only when a rule genuinely says nothing', () => {
    expect(ruleName(blankRule(1))).toBe('new rule')
  })
})

describe('ruleMechanism', () => {
  it('names a builtin by what it sets, which its kind states outright', () => {
    expect(
      ruleMechanism(aRule({ kind: 'builtin-volume', match_on: 'name' })),
    ).toBe('builtin on name → volume')
    expect(
      ruleMechanism(aRule({ kind: 'builtin-chapter', match_on: 'name' })),
    ).toBe('builtin on name → chapter')
  })

  it('names the fields a rule sets outright, in field order', () => {
    expect(
      ruleMechanism(
        aRule({
          kind: 'regex',
          match_on: 'path',
          set_series: 'A',
          set_language: 'en',
        }),
      ),
    ).toBe('regex on path → series, language')
  })

  it('claims nothing for a regex whose effect is in its capture groups', () => {
    // Saying "→ series" here would mean parsing the pattern and
    // reimplementing the server's derive in the client — the shape O93
    // closed. The line stops at what the rule states.
    expect(
      ruleMechanism(
        aRule({
          kind: 'regex',
          match_on: 'path',
          pattern: '^(?<series>[^/]+)/',
        }),
      ),
    ).toBe('regex on path')
  })

  it('is never empty, whatever the rule holds', () => {
    expect(ruleMechanism(blankRule(1))).toBe('regex on path')
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd web && npx vitest run --project node src/rules.test.ts -t 'ruleName'`
Expected: FAIL — `ruleName is not a function` (or a TypeScript error that the module has no such export).

- [ ] **Step 3: Implement both in `web/src/rules.ts`**

Append after `blankRule`:

```ts
/** The first of `values` that is neither `null` nor empty. */
function firstNamed(...values: Array<string | null>): string | undefined {
  return values.find((value): value is string => value !== null && value !== '')
}

/**
 * What a rule is called in the list.
 *
 * **`note` leads**, which it did not until ⑤a: the chain ran
 * `set_series ?? … ?? pattern || 'new rule'` and never consulted it, so a
 * `builtin-volume` or `builtin-chapter` rule — empty pattern, setting
 * nothing — read `new rule` permanently while carrying a perfectly good
 * note. Two of the four seeded rules did.
 *
 * **Empty strings fall through, not just `null`.** A `??` chain cannot see
 * `''`, and the old one's trailing `|| 'new rule'` caught it for the last
 * item only — so an empty note would have skipped the pattern and landed on
 * `new rule`. `firstNamed` treats the two the same the whole way down.
 */
export function ruleName(rule: Rule): string {
  return (
    firstNamed(
      rule.note,
      rule.set_series,
      rule.set_edition_key,
      rule.set_kind,
      rule.set_language,
      rule.pattern,
    ) ?? 'new rule'
  )
}

/**
 * The quiet line under a rule's name: how it matches, and what it sets.
 *
 * **Only what the rule states outright.** A `builtin-volume` kind sets
 * volume and a non-null `set_series` sets series; both are facts about the
 * row. A regex that sets a field through a named capture group is *not* —
 * saying so would mean parsing the pattern and reimplementing the server's
 * derive here, which is the second-opinion-in-the-client shape O93 closed.
 * `Row.decided_by` is the server's own answer and exists only once a
 * preview has landed, so a line built from it would appear and disappear.
 * This one never does.
 */
export function ruleMechanism(rule: Rule): string {
  const how = rule.kind.startsWith('builtin-') ? 'builtin' : rule.kind
  const sets =
    rule.kind === 'builtin-volume'
      ? ['volume']
      : rule.kind === 'builtin-chapter'
        ? ['chapter']
        : (
            [
              [rule.set_series, 'series'],
              [rule.set_edition_key, 'edition'],
              [rule.set_kind, 'kind'],
              [rule.set_language, 'language'],
            ] as const
          )
            .filter(([value]) => value !== null && value !== '')
            .map(([, name]) => name)
  return sets.length > 0
    ? `${how} on ${rule.match_on} → ${sets.join(', ')}`
    : `${how} on ${rule.match_on}`
}
```

- [ ] **Step 4: Run them and watch them pass**

Run: `cd web && npx vitest run --project node src/rules.test.ts`
Expected: PASS, whole file green.

- [ ] **Step 5: Write the failing row test in `web/src/RuleList.test.tsx`**

```tsx
  it('names a builtin by its note rather than calling it a new rule', () => {
    const { host } = render({
      rules: [
        rule({
          kind: 'builtin-volume',
          match_on: 'name',
          pattern: '',
          set_series: null,
          note: 'volume from the name',
        }),
      ],
    })
    const row = host.querySelector('.rule-row')
    expect(row?.querySelector('.select')?.textContent).toBe(
      'volume from the name',
    )
    expect(row?.querySelector('.mechanism')?.textContent).toBe(
      'builtin on name → volume',
    )
  })
```

- [ ] **Step 6: Run it and watch it fail**

Run: `cd web && npx vitest run --project jsdom src/RuleList.test.tsx -t 'names a builtin by its note'`
Expected: FAIL — the `.select` button reads `new rule`, and there is no `.mechanism` element.

- [ ] **Step 7: Use both in `web/src/RuleList.tsx`**

Add `ruleMechanism, ruleName` to the existing `from './rules'` import, then replace the `.select` button's children:

```tsx
            <button
              type="button"
              className="select"
              disabled={frozen}
              onClick={() => onSelect(index)}
            >
              <span className="rule-name">{ruleName(rule)}</span>
              <span className="mechanism">{ruleMechanism(rule)}</span>
            </button>
```

- [ ] **Step 8: Run the whole file and watch it pass**

Run: `cd web && npx vitest run --project jsdom src/RuleList.test.tsx`
Expected: PASS. If an older assertion fails because it read the button's whole `textContent`, update it to read `.rule-name` — the button now holds two strings.

- [ ] **Step 9: Hand-apply the defect and watch the test fail**

Per §5 item 3, and because this is a chain whose order is invisible to reading. In `rules.ts`, move `rule.note` from the first argument of `firstNamed` to the last. Run Step 4's command. Expected: FAIL on 'prefers the note'. **Restore the file** and re-run to confirm green.

This fixture is the one that matters: a rule with a note *and* a pattern renders identically under either order, so the test only reaches the defect because the builtin has an empty pattern.

- [ ] **Step 10: Commit**

```bash
git add web/src/rules.ts web/src/rules.test.ts web/src/RuleList.tsx web/src/RuleList.test.tsx
git commit -m "Rules: name a rule by its note, and say how it matches

The label chain never consulted \`note\`, so a builtin — empty pattern,
setting nothing — read \`new rule\` permanently; two of the four seeded
rules did. \`ruleName\` puts the note first and treats an empty string the
way it treats null, which the old \`??\` chain did not.

\`ruleMechanism\` adds the quiet second line, and claims only what a rule
states outright. A regex setting a field through a named capture group is
left unclaimed: inferring it would reimplement the server's derive in the
client."
```

---

### Task 3: The frame — the preview leaves the right pane

`editor-panes` is a bare `<div>`. At 1440 it, `.rule-list` and `.editor-right` each measure 1168 px of `<main>`'s 1216 and simply stack; a rule row's children end at x=584, leaving 856 px of every row empty. The list goes beside the form; the preview spans the width beneath both, because Provenance is the widest thing this pass has had to place.

**Files:**
- Modify: `web/src/RuleEditor.tsx:637-760`
- Modify: `web/src/styles/admin.css` — new section after the `.config-nav` block (currently ends around line 661)
- Test: `web/src/RuleEditor.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: the DOM shape `.rule-editor > .editor-panes > (.rule-list, .editor-right)` and `.rule-editor > .preview`. Tasks 6 and 7 style inside `.preview` and rely on it no longer being a descendant of `.editor-right`.

- [ ] **Step 1: Write the failing test in `web/src/RuleEditor.test.tsx`**

```tsx
  it('puts the preview outside the right pane, so it spans the width', () => {
    const { host } = render()
    const preview = host.querySelector('.preview')
    expect(preview).not.toBeNull()
    // The point of the move: Provenance is file + one column per metadata
    // field, and inside `.editor-right` it is capped at whatever the list
    // column leaves at every width.
    expect(host.querySelector('.editor-right .preview')).toBeNull()
    expect(preview?.parentElement?.className).toContain('rule-editor')
  })
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx -t 'puts the preview outside'`
Expected: FAIL — `.editor-right .preview` matches.

- [ ] **Step 3: Move the node in `web/src/RuleEditor.tsx`**

Close `</div>` for `.editor-right` immediately after the `RuleForm`/`.empty` ternary, and close `</div>` for `.editor-panes` there too, so `<PreviewTabs …/>` and everything after it sit as siblings of `.editor-panes` inside `.rule-editor`. Move no props and change no handlers — `PreviewTabs`'s entire prop list travels with it unchanged.

The resulting shape:

```tsx
      <div className="editor-panes">
        <RuleList … />
        <div className="editor-right">
          {selected !== undefined && rules[selected] !== undefined ? (
            <RuleForm … />
          ) : (
            <p className="empty">
              Pick a rule to edit it. The impact below is for the whole list.
            </p>
          )}
        </div>
      </div>

      <PreviewTabs … />
```

- [ ] **Step 4: Run the whole file and watch it pass**

Run: `cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx`
Expected: PASS. Existing tests that query `.preview` by itself still pass; any that query `.editor-right .preview` are asserting the old shape and should be updated to `.preview`.

- [ ] **Step 5: Add the frame's rules to `web/src/styles/admin.css`**

Insert immediately after the `.config-nav a[aria-current="page"]` rule:

```css
/* The rule editor — spec 2026-09-17 §10.3.
 *
 * **The preview is a sibling of the panes, not inside the right one.** The
 * class names promised two panes and drew none: `.editor-panes`,
 * `.rule-list` and `.editor-right` were block `<div>`s that each measured
 * 1168px of `<main>`'s 1216 and stacked, while a rule row's own children
 * ended at x=584 — 856px of every row empty. So what was missing was a grid
 * over full-width blocks, not width for narrow ones.
 *
 * The list goes beside the form because they are what you edit together.
 * The preview goes underneath both because it is the only part that needs
 * width: Provenance is *file plus one column per metadata field*, 8 columns
 * on a library with 4 rules, and it is the one thing here that cannot be
 * given width later. */
.rule-editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.rule-editor > h2 {
  margin: 0;
  font-size: var(--text-h2);
}

.editor-panes {
  display: grid;
  gap: var(--space-5);
  align-items: start;
}

/* One column below the breakpoint, and the source order — list, form — is
 * already the order a phone wants, so nothing is reordered. */
@media (min-width: 900px) {
  .editor-panes {
    grid-template-columns: minmax(14rem, 22rem) minmax(0, 1fr);
  }
}

/* `minmax(0, 1fr)` above, and `min-width: 0` here: a grid item defaults to
 * `min-width: auto` and refuses to shrink below its content, and this pane
 * holds a `pattern` field whose value is longer than any column. The same
 * reason `.proposal-list li` carries it. */
.editor-right {
  min-width: 0;
}

.editor-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}
```

- [ ] **Step 6: Run the stylesheet's own consumers**

Run: `cd web && npx vitest run --project node src/index.css.test.ts src/colourLiterals.test.ts`
Expected: PASS. `colourLiterals` must still report zero for `admin.css` — every rule above is tokens or geometry.

- [ ] **Step 7: Commit**

```bash
git add web/src/RuleEditor.tsx web/src/RuleEditor.test.tsx web/src/styles/admin.css
git commit -m "Rules: two panes, and the preview spanning beneath them

\`editor-panes\` was a bare div, so the three blocks each measured 1168px of
main's 1216 and stacked; the content stopped at x=584. The list now sits
beside the form and the preview moves out of \`.editor-right\` to span the
width, because Provenance is file plus one column per metadata field and is
the one part of this screen that cannot be given width later."
```

---

### Task 4: The rule list drawn

Two lines per row, the ordinal printed once, the selected row marked. `.rule-row` already carries `aria-current="true"` for the selected rule, so the CSS has a hook and needs no markup change.

**Files:**
- Modify: `web/src/styles/admin.css` (append to the section Task 3 opened)
- Modify: `web/src/RuleList.tsx` (the `.select` button, Step 0 only)
- Create: `web/src/styles/admin-rules.css.test.ts`
- Test: the new file, and `web/src/RuleList.test.tsx`

**Interfaces:**
- Consumes: `.rule-name` and `.mechanism` from Task 2; `.editor-panes` from Task 3.
- Produces: nothing later tasks rely on.

- [ ] **Step 0: Fix the button's accessible name — a Task 2 review finding**

Task 2 put two spans inside the `.select` button, and JSX strips the
whitespace-only text node between sibling elements, so the button's
accessible name is the two strings run together: a screen reader announces
`"volume from the namebuiltin on name → volume"`. **CSS cannot be relied on
to fix this.** Making the spans block-level does cause browsers to insert a
space during accessible-name computation, but that is a rendering-dependent
side effect, not a guarantee, and this task's own grid would be the only
thing standing between a correct name and a run-on one.

Give the button an explicit label instead. In `web/src/RuleList.tsx`, add one
attribute to the `.select` button — change nothing else about it:

```tsx
            <button
              type="button"
              className="select"
              disabled={frozen}
              aria-label={`${ruleName(rule)} — ${ruleMechanism(rule)}`}
              onClick={() => onSelect(index)}
            >
```

Write this test first, in `web/src/RuleList.test.tsx`, and watch it fail:

```tsx
  it('gives the row button an accessible name that is not two strings run together', () => {
    const { host } = render({
      rules: [
        rule({
          kind: 'builtin-volume',
          match_on: 'name',
          pattern: '',
          set_series: null,
          note: 'volume from the name',
        }),
      ],
    })
    const button = host.querySelector('.select')
    expect(button?.getAttribute('aria-label')).toBe(
      'volume from the name — builtin on name → volume',
    )
    // WCAG 2.5.3, Label in Name: whatever a person sees on the control has to
    // be inside what a screen reader announces for it, or a voice-control user
    // cannot ask for it by the name they can read.
    const visible = button?.querySelector('.rule-name')?.textContent ?? ''
    expect(visible).not.toBe('')
    expect(button?.getAttribute('aria-label')).toContain(visible)
  })
```

Run: `cd web && npx vitest run --project jsdom src/RuleList.test.tsx -t 'not two strings run together'`
Expected before the fix: FAIL — `expected null to be 'volume from the name — builtin on name → volume'`.
After the fix: PASS. Commit this together with the rest of the task.

- [ ] **Step 1: Write the failing test — create `web/src/styles/admin-rules.css.test.ts`**

```ts
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
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts`
Expected: FAIL — both `[]`, because no such rules exist yet.

- [ ] **Step 3: Add the list's rules to `web/src/styles/admin.css`**

```css
/* The rule list — §10.4. An `<ol>` whose markers are off because the row
 * prints its own ordinal: together they drew `1. 1`, `2. 2` down the list. */
.rule-list ol {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: var(--space-1);
}

.rule-row {
  display: flex;
  align-items: start;
  gap: var(--space-2);
  padding: var(--space-2);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
}

/* Marked with an inset shadow rather than a border, for the reason
 * `.proposal-list li.strong` gives: a border that changes between rows
 * shifts the content beside it. The transparent border above holds the
 * space so the row does not move when it gains a visible one. */
.rule-row[aria-current="true"] {
  background: var(--surface);
  border-color: var(--line-strong);
  box-shadow: inset 3px 0 0 var(--accent);
}

.rule-row .ordinal {
  color: var(--muted);
  font-size: var(--text-meta);
  font-variant-numeric: tabular-nums;
  min-width: 1.25em;
  text-align: right;
}

/* The disclosure is the whole line, so it stops looking like a button — the
 * same move `.proposals .row .disclosure` makes, and for the same reason. */
.rule-row .select {
  flex: 1;
  min-width: 0;
  display: grid;
  gap: 1px;
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.rule-row .rule-name {
  overflow-wrap: anywhere;
}

/* Quiet by size and colour, never by `opacity` — the rule this stylesheet's
 * `.proposal-detail` comment sets out, and the reason `index.css.test.ts`
 * exists. */
.rule-row .mechanism {
  color: var(--muted);
  font-size: var(--text-meta);
}

.rule-row .off,
.rule-row .from-proposal {
  align-self: center;
  padding: 0 var(--space-2);
  border: 1px solid var(--line-strong);
  border-radius: 999px;
  color: var(--muted);
  font-size: var(--text-meta);
  white-space: nowrap;
}

.rule-list .rule-count {
  margin: var(--space-3) 0 0;
  color: var(--muted);
  font-size: var(--text-ui);
}

.rule-list .cost {
  margin: var(--space-2) 0 0;
  font-size: var(--text-meta);
}
```

- [ ] **Step 4: Run it and watch it pass**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts`
Expected: PASS, both cases.

- [ ] **Step 5: Hand-apply each defect and watch the test fail**

Per §5 item 3, one at a time, restoring between:

1. Change `list-style: none` to `list-style: decimal`. Run Step 4. Expected: FAIL on 'prints each ordinal once'.
2. Replace the `box-shadow` on `.rule-row[aria-current="true"]` with `border-left-width: 3px`. Run Step 4. Expected: FAIL on 'marks the selected row'.

Restore the file and re-run to confirm green.

- [ ] **Step 6: Commit**

```bash
git add web/src/styles/admin.css web/src/styles/admin-rules.css.test.ts web/src/RuleList.tsx web/src/RuleList.test.tsx
git commit -m "Rules: draw the list — two lines a row, one ordinal, a marked selection

The list is an ol that also printed its own ordinal, so every row read
\`1. 1\`. The selected row is marked with an inset shadow rather than a
border, because a border present on one row and absent on the next shifts
the column beside it — the same reason \`.proposal-list li.strong\` uses one.

\`admin-rules.css.test.ts\` pins only the declarations whose absence is a defect;
how the screen looks is the browser pass's job."
```

---

### Task 5: The form adopts `.field`, hoisted

`.proposal-detail .field` was built by C3 for `AcceptForm`, which creates a rule from a proposal — the same content this form edits. Its comment records, measured, why `.field.pattern` takes a row to itself: sharing a row gave the pattern 416 px of the form's 807 at a 1100 px window and cut a 52-character `rel_path` prefix.

**Files:**
- Modify: `web/src/styles/admin.css:488-527` (the `.proposal-detail .field` block) and the `.account input` `biome-ignore` above it
- Modify: `web/src/RuleForm.tsx:97-172` (add `className="field"` to each label)
- Test: `web/src/styles/admin-rules.css.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: a `.field` class usable on any screen in `admin.css`. ⑤c's `AcceptForm` keeps working through it.

- [ ] **Step 1: Write the failing test in `web/src/styles/admin-rules.css.test.ts`**

```ts
describe('the field pattern', () => {
  it('is a shared class, not one screen’s', () => {
    // C3 built it for AcceptForm; ⑤a's rule form edits the same content, so
    // it is hoisted rather than copied.
    expect(bodies('.field')).toHaveLength(1)
    expect(bodies('.proposal-detail .field')).toHaveLength(0)
  })

  it('keeps the pattern field on a row of its own', () => {
    // Measured in C3: sharing the row gave the pattern 416px of 807 at a
    // 1100px window and cut a 52-character rel_path prefix.
    expect(declared('.field.pattern', 'flex')).toEqual(['10100%'])
  })

  it('draws the rule form’s controls through it', () => {
    expect(bodies('.rule-form')).toHaveLength(1)
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts -t 'the field pattern'`
Expected: FAIL — `.field` has no rule and `.proposal-detail .field` has one.

- [ ] **Step 3: Hoist the selectors in `web/src/styles/admin.css`**

Rewrite the prelude of each of these five rules, leaving every declaration exactly as it is:

| from | to |
|---|---|
| `.proposal-detail .field` | `.field` |
| `.proposal-detail .field.pattern` | `.field.pattern` |
| `.proposal-detail .field input,`<br>`.proposal-detail .field select` | `.field input,`<br>`.field select` |
| `.proposal-detail .field.pattern input` | `.field.pattern input` |

Leave `.proposal-detail .rule-fields` and `.proposal-detail .accept-form .scope` alone — those are the inbox's own layout, not the field pattern.

- [ ] **Step 4: Settle the `biome-ignore`, do not leave it**

The comment above `.account input` justifies its `noDescendingSpecificity` ignore by saying `.account` and `.proposal-detail` are never drawn inside one another, so their order decides nothing. A shared `.field` reaching a third screen changes that premise. Replace the comment body:

```css
/* biome-ignore lint/style/noDescendingSpecificity: the rule Biome weighs
   this against is `.field input`, above — since ⑤a a shared class rather
   than `.proposal-detail`'s own. The two still never compete: `.account`
   draws no `.field`, and `.field` is only ever inside the inbox's accept
   form or the rule editor's form, neither of which is inside `.account`.
   So their order decides nothing, which is what the rule is about. */
```

Then confirm Biome agrees the ignore is still *needed* — an unused ignore is itself an error:

Run: `cd web && npm run lint`
Expected: no diagnostics. If Biome reports the suppression is unused, delete the comment rather than keeping a lie.

- [ ] **Step 5: Put the class on the rule form's labels in `web/src/RuleForm.tsx`**

In the `text()` helper: `<label className={name === 'pattern' ? 'field pattern' : 'field'}>`.
On each of the three `<select>` labels — `kind`, `match on`, `scope` — add `className="field"`.

- [ ] **Step 6: Add the form's own layout to `web/src/styles/admin.css`**

```css
/* The rule form — §10.5. Nine controls on the `.field` pattern above, which
 * C3 built for the accept form: the same content, edited from the other
 * side. Four across where there is room, two on a phone; `pattern` takes a
 * row of its own through `.field.pattern`.
 *
 * Its predecessor was an inline flow with no rules at all, which at 390px
 * put each label beside the *previous* field's input — the group read
 * `series [ ] edition / [ ] kind / [ ] language / [ ]`. The markup was
 * right and the rendering was not, which is the worse way round. */
.rule-form {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.rule-form .field {
  flex: 1 1 8rem;
}

.rule-form .rule-sets {
  flex: 1 0 100%;
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin: 0;
  padding: var(--space-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
}

.rule-form .rule-sets legend {
  padding: 0 var(--space-1);
  color: var(--muted);
  font-size: var(--text-meta);
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
```

- [ ] **Step 7: Run the tests and watch them pass**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts src/index.css.test.ts && npx vitest run --project jsdom src/AcceptForm.test.tsx`
Expected: PASS. `AcceptForm` is the screen the hoist touches; if any of its tests read `.proposal-detail .field` as a selector, update them to `.field`.

- [ ] **Step 8: Run AcceptForm's browser test, which measures the hoisted boxes**

Run: `cd web && npx vitest run --project browser src/AcceptForm.browser.test.tsx`
Expected: PASS. This file exists precisely because the field layout was measured rather than chosen, so it is the one that would catch a hoist that changed it.

- [ ] **Step 9: Commit**

```bash
git add web/src/styles/admin.css web/src/styles/admin-rules.css.test.ts web/src/RuleForm.tsx
git commit -m "Rules: hoist AcceptForm's .field and lay the rule form out on it

C3 built \`.proposal-detail .field\` for the accept form, which creates a
rule from a proposal — the same content this form edits from the other
side. It is hoisted to a plain \`.field\` rather than copied, and the
\`noDescendingSpecificity\` ignore that named the old selector is
re-justified against the new one rather than left behind.

The form was an inline flow with no rules, which at 390px put each label
beside the previous field's input."
```

---

### Task 6: One tab look for every one-of-N row

`preview-tabs` and `config-nav` take the underlined tab ② built for the library's filter row. The line this draws is one-of-N against independent toggle, not navigation against panel — which is the distinction ②'s whole-branch review found when the tab look reached the inbox and "All" and "Hide weak proposals" drew the accent underline side by side.

**Files:**
- Modify: `web/src/styles/admin.css` (the `.config-nav` block, and the new section)
- Test: `web/src/styles/admin-rules.css.test.ts`

**Interfaces:**
- Consumes: `.preview` as a top-level child of `.rule-editor` (Task 3).
- Produces: nothing later tasks rely on.

- [ ] **Step 1: Write the failing test in `web/src/styles/admin-rules.css.test.ts`**

```ts
describe('the tab look', () => {
  it('underlines the current tab without moving the row', () => {
    // `inset 0 -2px 0`, not a border: ②'s `.chips.library-chips` draws it
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
```

**There is deliberately no case here for the inbox keeping its pills.**
`index.css.test.ts` already owns that — its `describe('the chips keep their
pills everywhere but the library's own row')` — and those rules live in
`library.css`, which this file does not read. A second assertion here would
read the wrong stylesheet and pass vacuously, which is worse than none.
Step 4 runs the existing one instead.

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts -t 'the tab look'`
Expected: FAIL — neither prelude declares a `box-shadow`.

- [ ] **Step 3: Rewrite `.config-nav` and add `.preview-tabs` in `web/src/styles/admin.css`**

Replace the existing `.config-nav ul` and `.config-nav a[aria-current="page"]` rules with:

```css
/* The settings sub-nav and the preview strip wear one look — §10.6.
 *
 * The line is **one-of-N against independent toggle**, not navigation
 * against panel: a row of links and a row of buttons are different markup
 * for the same act, and a reader choosing among four does not care which
 * changed the URL. The inbox's filters keep their pills because one of them
 * is not a tab — ②'s whole-branch review found "All" and "Hide weak
 * proposals" drawing the same accent underline side by side, which is what
 * pulled the look back off that row.
 *
 * `inset 0 -2px 0` rather than a border-bottom, copying
 * `.chips.library-chips`: a border appearing under the current item moves
 * the row's baseline every time the selection changes. */
.config-nav ul {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-5);
  list-style: none;
  margin: 0 0 var(--space-5);
  padding: 0 0 var(--space-1);
  border-bottom: 1px solid var(--line);
}

.config-nav a {
  color: var(--muted);
  font-size: var(--text-ui);
  text-decoration: none;
}

.config-nav a[aria-current="page"] {
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}

.preview-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-5);
  margin-bottom: var(--space-4);
  padding-bottom: var(--space-1);
  border-bottom: 1px solid var(--line);
}

.preview-tabs button {
  padding: var(--space-1) 0;
  border: 0;
  border-radius: 0;
  background: none;
  color: var(--muted);
  font: inherit;
  font-size: var(--text-ui);
  cursor: pointer;
}

.preview-tabs button[aria-current="true"] {
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}

.preview-tabs .badge {
  padding: 0 var(--space-2);
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: var(--text-meta);
}
```

- [ ] **Step 4: Run it and watch it pass**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts src/index.css.test.ts`
Expected: PASS. `index.css.test.ts` checks contrast over `adminSource`, so `--on-accent` on `--accent` is verified there rather than here.

- [ ] **Step 5: Hand-apply the defect and watch the test fail**

Change `.preview-tabs button[aria-current="true"]`'s `box-shadow` to `border-bottom: 2px solid var(--accent)`. Run Step 4. Expected: FAIL on 'underlines the current tab without moving the row'. Restore and re-run.

- [ ] **Step 6: Commit**

```bash
git add web/src/styles/admin.css web/src/styles/admin-rules.css.test.ts
git commit -m "Rules: one tab look for every one-of-N row

The preview strip and the settings sub-nav take the underlined tab ② built
for the library's filter row. The line is one-of-N against independent
toggle, not navigation against panel — which is what ②'s review found when
the look reached the inbox and a filter and a toggle drew the same accent
underline side by side. The inbox keeps its pills.

Drawn as \`inset 0 -2px 0\` rather than a border-bottom, so changing tab
does not move the row's baseline."
```

---

### Task 7: The tables scroll in their own box, with `file` pinned

Both preview tables overflow at 390 px: `table.provenance` renders 524 px against a 375 px client width, `table.matched` 393 px, for 165 px and 34 px of document overflow. Only one is in the DOM at a time, so a single-tab assertion would pass over the other.

**Files:**
- Modify: `web/src/PreviewTabs.tsx:217` and `:249` (wrap each `<table>`)
- Modify: `web/src/styles/admin.css`
- Create: `web/src/PreviewTabs.browser.test.tsx`
- Test: the new file, and `web/src/styles/admin-rules.css.test.ts`

**Interfaces:**
- Consumes: `.preview` outside `.editor-right` (Task 3); `.preview-tabs` (Task 6).
- Produces: a `.table-scroll` wrapper around each preview table. ⑤b may reuse it for the scan report.

- [ ] **Step 1: Write the failing CSS test in `web/src/styles/admin-rules.css.test.ts`**

```ts
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
    }
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts -t 'the preview tables'`
Expected: FAIL — no such rules.

- [ ] **Step 3: Wrap both tables in `web/src/PreviewTabs.tsx`**

Around the `<table className="matched">` and the `<table className="provenance">`, add `<div className="table-scroll">…</div>`. Change nothing inside either table.

- [ ] **Step 4: Add the rules to `web/src/styles/admin.css`**

```css
/* A wide table scrolls inside its own box, and the page never does — §10.6,
 * the pattern ② built for the series table (`library.css`'s
 * `.table-scroll`, spec §7.2).
 *
 * Measured before this existed, at a 390px viewport where driven Chromium
 * leaves 375px of client width: `.provenance` rendered 524px and `.matched`
 * 393px, for 165px and 34px of document overflow. Only one is ever in the
 * DOM, so each was found on its own tab.
 *
 * **`file` is pinned**, because scrolling the fields past it otherwise
 * costs the reader the row they are on — and Provenance's rows are file
 * paths that differ in their last component. A sticky cell needs a
 * background of its own or the cells it slides over show through; that is
 * the half of this that looks correct while being unreadable, and
 * `admin-rules.css.test.ts` pins it for that reason. */
.preview .table-scroll {
  max-width: 100%;
  overflow-x: auto;
}

.preview table {
  border-collapse: collapse;
  width: 100%;
}

.preview th {
  text-align: left;
  font-weight: 500;
  font-size: var(--text-meta);
  color: var(--muted);
  border-bottom: 1px solid var(--line-strong);
}

.preview th,
.preview td {
  padding: var(--space-1) var(--space-3);
  white-space: nowrap;
}

.preview td {
  border-bottom: 1px solid var(--line);
  font-size: var(--text-ui);
}

.preview .table-scroll th:first-child,
.preview .table-scroll td:first-child {
  position: sticky;
  left: 0;
  background: var(--bg);
}

/* The hairline the pinned column would otherwise lose: `border-bottom` sits
 * under the cell, and nothing draws its right edge once it is floating over
 * the columns it has covered. */
.preview .table-scroll td:first-child {
  box-shadow: 1px 0 0 var(--line);
}
```

- [ ] **Step 5: Run it and watch it pass**

Run: `cd web && npx vitest run --project node src/styles/admin-rules.css.test.ts`
Expected: PASS, whole file.

- [ ] **Step 6: Write the browser test — create `web/src/PreviewTabs.browser.test.tsx`**

```tsx
import { describe, expect, it } from 'vitest'
// **Required**, the same reason `Filmstrip.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app
// through `index.css`. Without this the boxes below are unstyled and the
// assertions measure browser defaults.
import './index.css'
import { PreviewTabs } from './PreviewTabs'
import { mount } from './testing/mount'

/**
 * What jsdom cannot answer: whether the table scrolls instead of the page,
 * and whether the pinned column is actually opaque. Every assertion here is
 * a measured box or a computed style.
 */

/**
 * Real paths, because the widths under test are their widths. `FIELDS` in
 * `PreviewTabs.tsx` is a seven-element constant, so Provenance is 8 columns
 * whatever the data holds; what the rows have to supply is `rel_path`
 * strings as long as a real library's, which is what pushed the table to
 * 524px against a 375px client width.
 */
const PATHS = [
  'Death Note (Colored Council)/Death Note - v01 (Colored Council).cbz',
  'Death Note (Colored Council)/Death Note - v02 (Colored Council).cbz',
  'Jigokuraku - Side Story - Forest of Misfortune/Jigokuraku - Side Story - Forest of Misfortune (2023).cbz',
]

const rows: Row[] = PATHS.map((rel_path, i) => ({
  rel_path,
  root_id: 1,
  before: { series: 'Death Note', work: `w${i}`, edition: 'en' },
  after: { series: 'Death Note', work: `w${i}`, edition: 'en' },
  matched: i !== 2,
  decided_by: {
    series: { kind: 'rule', index: 0 },
    volume: { kind: 'rule', index: 1 },
  },
}))

const fixture = {
  preview: {
    impact: {
      series_before: 134,
      series_after: 134,
      series_merged: 0,
      works_rehomed: 0,
      works_orphaned: 0,
      proposals_reopened: 0,
      proposals_raised: 0,
    },
    rows,
    rows_truncated: false,
    overrides: [],
  } satisfies Preview,
  rules: [] as Rule[],
  selected: 0,
  busy: false,
  frozen: false,
  onReorder: () => {},
  onSelectRule: () => {},
}

const settle = () =>
  new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)))

function open(host: HTMLElement, label: string) {
  const button = [
    ...host.querySelectorAll<HTMLButtonElement>('.preview-tabs button'),
  ].find((b) => b.textContent?.startsWith(label))
  if (!button) throw new Error(`no tab named ${label}`)
  button.click()
}

describe('the preview at phone width', () => {
  it('never moves the page sideways, on any of the four tabs', async () => {
    document.documentElement.style.width = '390px'
    const host = mount(<PreviewTabs {...fixture} />)
    // **All four, not just the two with tables.** Only one tab's content is
    // in the DOM at a time, and both tables overflowed before this — by
    // 165px and 34px — so a single-tab assertion passes over whichever one
    // it did not open.
    for (const tab of ['Impact', 'Matched files', 'Provenance', 'Overrides']) {
      open(host, tab)
      await settle()
      const doc = document.documentElement
      expect(doc.scrollWidth, tab).toBeLessThanOrEqual(doc.clientWidth)
    }
  })

  it('scrolls the table inside its own box instead', async () => {
    document.documentElement.style.width = '390px'
    const host = mount(<PreviewTabs {...fixture} />)
    for (const tab of ['Matched files', 'Provenance']) {
      open(host, tab)
      await settle()
      const box = host.querySelector('.table-scroll') as HTMLElement
      expect(box, tab).not.toBeNull()
      expect(box.scrollWidth, tab).toBeGreaterThan(box.clientWidth)
    }
  })

  it('keeps the file column on screen, and opaque, through the scroll', async () => {
    document.documentElement.style.width = '390px'
    const host = mount(<PreviewTabs {...fixture} />)
    open(host, 'Provenance')
    await settle()
    const box = host.querySelector('.table-scroll') as HTMLElement
    const firstCell = box.querySelector('tbody td:first-child') as HTMLElement
    const before = firstCell.getBoundingClientRect().left
    box.scrollLeft = box.scrollWidth - box.clientWidth
    await settle()
    expect(firstCell.getBoundingClientRect().left).toBeCloseTo(before, 0)
    // Position alone is not enough: a transparent sticky cell sits in
    // exactly the right place while the columns it covers read straight
    // through it.
    const background = getComputedStyle(firstCell).backgroundColor
    expect(background).not.toBe('rgba(0, 0, 0, 0)')
    expect(background).not.toBe('transparent')
  })
})
```

Add `import type { Preview, Row, Rule } from './rules'` to the imports at the top of the file.

- [ ] **Step 7: Run the browser test and watch it pass**

Run: `cd web && npx vitest run --project browser src/PreviewTabs.browser.test.tsx`
Expected: PASS, all three cases.

If 'never moves the page sideways' passes on every tab *before* Step 4's CSS is in place, the fixture is not reproducing the overflow — lengthen `PATHS` until it does, and confirm it fails, before trusting it.

- [ ] **Step 8: Hand-apply both defects and watch each fail**

1. Delete `background: var(--bg)` from the sticky rule. Run Steps 5 and 7. Expected: `admin-rules.css.test.ts` FAILS on the background assertion, and the browser test FAILS on `backgroundColor`. **Both must fail** — if only the CSS-text one does, the browser assertion is not reaching the defect.
2. Change `.preview .table-scroll`'s `overflow-x` to `visible`. Run Step 7. Expected: **two** failures — 'never moves the page sideways' on the Matched files and Provenance tabs, and 'scrolls the table inside its own box instead', where `scrollWidth` now equals `clientWidth` because nothing is clipping. If only one fails, say which and why before moving on.

Restore and re-run.

- [ ] **Step 9: Commit**

```bash
git add web/src/PreviewTabs.tsx web/src/PreviewTabs.browser.test.tsx web/src/styles/admin.css web/src/styles/admin-rules.css.test.ts
git commit -m "Rules: the preview tables scroll in their own box, file pinned

Measured at a 390px viewport, where driven Chromium leaves 375px of client
width: provenance rendered 524px and matched 393px, for 165px and 34px of
document overflow. Only one is in the DOM at a time, so the assertion walks
both tabs — a single-tab check passes over the 34px one.

\`file\` is sticky, and its background is pinned by a test of its own: a
transparent sticky cell sits in exactly the right place while the columns it
covers read straight through it, which no geometry assertion can see."
```

---

### Task 8: The preview tables take the UI face

§7.2 left `base.css`'s blanket table rule in place "because the rule preview's tables are ⑤'s question". Neither table has a column of figures: Matched files draws a path, two series names and a word; Provenance a path and cells reading `rule 1`, `root default` or `—`. Digits do occur inside those strings — 14 of this library's 138 series titles carry one — which is the point, since digits inside prose are what the figures face drew heavier than the words around them.

**Files:**
- Modify: `web/src/styles/admin.css`
- Modify: `web/src/index.css.test.ts:34` and its `--font-figures` describe block (around `:541`)
- Test: `web/src/index.css.test.ts`

**Interfaces:**
- Consumes: `.preview table` from Task 7.
- Produces: nothing later tasks rely on.

- [ ] **Step 1: Widen the existing check in `web/src/index.css.test.ts` and watch it fail**

`cssSource` today is `[shellSource, librarySource, adminSource].join('\n')`, so the check that `--font-figures` "is named by no other rule in the screens' stylesheets" cannot see `base.css`, which is where the blanket rule lives. Add the import and extend the describe block:

```ts
import baseSource from './styles/base.css?raw'
```

Then replace the second case of `describe('the figures face reaches the series table’s page counts only', …)`:

```ts
  it('is named by no other rule in the screens’ stylesheets', () => {
    // `base.css` is read here too since ⑤a. It carries the blanket rule
    // that puts the figures face on every table under `.library`, which is
    // exactly what this case exists to notice — and could not, while it
    // read only the three screen stylesheets.
    const naming = rules([cssSource, baseSource].join('\n'))
      .filter((rule) => rule.body.includes('--font-figures'))
      .map((rule) => rule.prelude)
    expect(naming).toEqual([
      '.editions td.pages',
      ':where(.library, .login) :where(table)',
    ])
  })

  it('does not reach the rule preview’s tables', () => {
    // §7.2 deferred this to ⑤. Neither table has a numeric column: every
    // digit in Matched files is inside a file name, and Provenance's only
    // digits are rule ordinals reading `rule 1`. So both opt out the way
    // `.editions` does — and unlike `.editions` they add no cell exception
    // back, because there is no page-count column to make one for.
    expect(declared(ruleBody('.preview table'), 'font-family')).toBe(
      'var(--font-ui)',
    )
    const exceptions = rules(adminSource)
      .filter(
        (rule) =>
          rule.prelude.startsWith('.preview') &&
          rule.body.includes('--font-figures'),
      )
      .map((rule) => rule.prelude)
    expect(exceptions).toEqual([])
  })
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run --project node src/index.css.test.ts -t 'figures face'`
Expected: FAIL on 'does not reach the rule preview's tables' — `.preview table` declares no `font-family`.

If the first case also fails, read what it reports: it is telling you the exact prelude `base.css` uses, and the expected array above should be corrected to match the stylesheet rather than the other way round.

- [ ] **Step 3: Add the opt-out to `web/src/styles/admin.css`**

Extend the `.preview table` rule Task 7 added:

```css
/* The UI face, with no cell exception — §10.6, answering the question §7.2
 * reserved for ⑤. `base.css` puts `--font-figures` and `tabular-nums` on
 * every table under `.library`; `.editions` opted back out in ② because the
 * digits inside a volume's file name drew heavier than its title. These two
 * tables are *almost entirely* file names, and neither has a numeric column
 * — Provenance's only digits are rule ordinals reading `rule 1` — so they
 * opt out the same way and add nothing back. */
.preview table {
  border-collapse: collapse;
  width: 100%;
  font-family: var(--font-ui);
}
```

- [ ] **Step 4: Run it and watch it pass**

Run: `cd web && npx vitest run --project node src/index.css.test.ts`
Expected: PASS, whole file.

- [ ] **Step 5: Hand-apply the defect and watch the test fail**

Delete the `font-family: var(--font-ui)` line. Run Step 4. Expected: FAIL on 'does not reach the rule preview's tables'. Restore and re-run.

- [ ] **Step 6: Commit**

```bash
git add web/src/styles/admin.css web/src/index.css.test.ts
git commit -m "Rules: the preview tables take the UI face, with no exception

§7.2 left base.css's blanket table rule in place because these two tables
were ⑤'s question. Neither has a numeric column — every digit in Matched
files is inside a file name, and Provenance's only digits are rule ordinals
reading \`rule 1\` — so both opt out the way .editions does, and unlike
.editions add no cell exception back.

index.css.test.ts now reads base.css too. Its check that --font-figures is
named by no other rule could not see the blanket rule while it read only the
three screen stylesheets."
```

---

## Before opening the PR

§5's checklist, in full. These are not optional and three of them have caught things nothing else did:

- [ ] **The gate, with its `all green` line quoted.**
  `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh`
  Do not pipe it through `tail` or append `; echo` — either masks the exit status. Quote the `all green` line verbatim.
- [ ] **A browser pass against a scratch copy of the real library**, at 390×844 and 1440×900, in Paper and in Ink. A copy, not the library: checksum `/home/davey/temp/manga-test/library.db` before and after and state that it is unchanged. Walk all four preview tabs at both widths in both themes.
- [ ] **Every new test watched failing with its defect put back by hand.** Tasks 2, 4, 6, 7 and 8 each have that step written out; do them, do not assume them.
- [ ] **A whole-branch review, and a review of each fix round until one finds nothing.** Per-task review is not a substitute: on #91 every task passed while a whole-branch review found seven Criticals.
- [ ] **The pre-merge claim check** over the PR body, `docs/status.md` and `docs/notes.md` — separately from the review, after the gate is green. On #169 it found nine wrong claims after both were clean.
- [ ] **File what was found and not fixed**, each as its own open item:
  - Hiding Provenance columns empty in every row. §10.7 has the counts: 4 of 8 columns carry 0 values across all 224 rows.
  - Whether the inbox's independent toggle should leave its tab row (⑤c's, §10.6).
- [ ] **Update `docs/status.md`** — the five-part edit, and the row count at the head of the open-items section, which has been eight behind before.

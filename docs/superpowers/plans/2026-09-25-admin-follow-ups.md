# Admin follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close O149, O153, O155, O157 and O159 on the admin screens as the
user decided on 2026-09-25, in one PR.

**Architecture:** Five small, independent changes to `web/src`: two pure
helpers in `rules.ts` and a filtered table in `PreviewTabs.tsx` (O149), one
selector in `admin.css` (O153), a three-valued `strength()` in
`curation.ts` (O155), a label and a painted-nothing `::after` under
`(pointer: coarse)` (O157), and `aria-disabled` in place of `disabled` on
`UserRow`'s checkboxes (O159). Then a browser pass, the `docs/status.md`
bookkeeping and the gate.

**Tech Stack:** React 19, TypeScript, plain CSS with custom properties,
Vitest in three projects: `node` (`src/**/*.test.ts`), `jsdom`
(`src/**/*.test.tsx` except `*.browser.test.tsx`) and `browser`
(`src/**/*.browser.test.tsx`, real Chromium). Biome for lint and format.
Playwright for the manual touch measurement.

**Spec:** `docs/superpowers/specs/2026-09-25-admin-follow-ups-design.md`.
Read it before starting. §1 holds the decisions and §2 the changes. Where
this plan and the spec disagree, stop and say so.

## Global Constraints

- **Branch `admin-follow-ups`**, already created from `main` at `e8eecfc5`.
  Commit at the end of every task. Do not push.
- **Every new or changed test is watched failing with its defect put back
  by hand** before the task is done. The step says which defect. A test
  that has never been red proves nothing here: this repo has shipped a
  dozen tests that passed against the defect they named.
- **Stylesheet assertions live in the `node` project.** The `jsdom` project
  stubs a `?raw` CSS import to `''`, so a `rules()` count there is silently
  zero (`web/src/testing/css.ts`'s head comment).
- **Run vitest with `--reporter=default`.** Under Claude Code it otherwise
  picks an `agent` reporter that hides passing output. From the repo root:
  `pnpm -C web exec vitest run --project node --reporter=default <files>`.
- **Before a task's commit, run the whole project the task touched, not
  only the named file.** A named-file run hid a broken test for 16 commits
  on #167.
- **Format and lint** with `pnpm -C web exec biome check --write <files>`
  and then `pnpm -C web run lint`. Biome sorts imports and wraps long
  preludes, so let it and don't hand-format.
- **The shell is zsh with `noclobber`**, and `cd` is zoxide. Use
  `pnpm -C web …` and absolute paths rather than `cd web && …`.
- **Scratch files go in the session scratchpad or `target/o-bundle/`**
  (gitignored), never in the working tree and never in `$HOME`.
- **The real library is read-only.**
  `sha256sum /home/davey/temp/manga-test/library.db` must print
  `d5ebc0c2378650356115a8cce637e50dc5b4ab7f7cc8f388553cadd575a59e67` before
  and after. The scratch copy is `target/o-bundle/lib/library.db`, account
  `local`, password `drawpw`.
- **Comments match the file around them**: this codebase explains *why* at
  length and names the O-number. `docs/status.md` names sites by symbol,
  never by line number.
- **No AI or tool attribution** in any file, comment or commit.
- **The gate** is `env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh`,
  run on its own with its last line quoted. Never pipe it through `tail`
  or chain `; echo`, because both mask its exit status.

---

## File Structure

| File | Change | Task |
|---|---|---|
| `web/src/curation.ts` | `strength` answers `'strong' \| 'maybe' \| 'weak'` | 1 |
| `web/src/curation.test.ts` | the three words, and the weak line pinned to `INBOX_MIN_SCORE` | 1 |
| `web/src/ProposalRow.test.tsx`, `web/src/Proposals.test.tsx` | a score-1 row reads `maybe` | 1 |
| `web/src/styles/admin.css` | the strength-strip comment (1); `.users .why` (2); `.preview .unset` and the editor comment (3); the coarse block's label and `::after` rules and comments (5, 6) | 1, 2, 3, 5, 6 |
| `web/src/styles/admin-config.css.test.ts` | the quiet rule reaches `.users .why` | 2 |
| `web/src/rules.ts` | `FIELDS`, `Field`, `unsetFields`, `unsetSentence` | 3 |
| `web/src/rules.test.ts` | the two helpers | 3 |
| `web/src/PreviewTabs.tsx` | draws only decided columns, the line above, the cap notice on Provenance | 3 |
| `web/src/PreviewTabs.test.tsx` | the table's columns, the line, the cap | 3 |
| `web/src/PreviewTabs.browser.test.tsx` | the fixture decides all seven fields so it stays the eight-column case | 3 |
| `web/src/UserRow.tsx` | checkboxes: `aria-disabled={saving}` | 4 |
| `web/src/UserRow.test.tsx` | the hold, asserted through the ref rather than the attribute | 4 |
| `web/src/RuleList.tsx` | the enable box inside `<label className="enable">` | 5 |
| `web/src/RuleList.test.tsx` | the label | 5 |
| `web/src/styles/admin-proposals.css.test.ts` | the coarse block's new rules | 5, 6 |
| `docs/notes.md` | O157's touch measurement | 7 |
| `docs/status.md` | five closes, one open, the counts | 8 |
| `docs/superpowers/specs/2026-09-17-style-pass-design.md` | one line under §12.8's O149 bullet | 8 |

---

## Task 0: Baseline

**Files:** none changed.

**Interfaces:**
- Consumes: nothing.
- Produces: the three baseline test counts later tasks are measured
  against, written to the scratchpad.

- [ ] **Step 1: Confirm the branch and a clean tree**

```bash
git -C /home/davey/projects/taiyaku branch --show-current
git -C /home/davey/projects/taiyaku status --short
git -C /home/davey/projects/taiyaku log --oneline -3
```

Expected: `admin-follow-ups`, no status lines, and the spec's commits on
top of `e8eecfc5`.

- [ ] **Step 2: Confirm the real library is untouched**

```bash
sha256sum /home/davey/temp/manga-test/library.db
```

Expected: `d5ebc0c2378650356115a8cce637e50dc5b4ab7f7cc8f388553cadd575a59e67`.
If it differs, stop and say so.

- [ ] **Step 3: Measure the baseline counts**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default 2>&1 | grep -E '^ +Tests '
```

Expected: `Tests  1362 passed (1362)` and `Tests  1121 passed (1121)`,
which are the figures in `docs/status.md`. Write both lines to
`<scratchpad>/baseline.txt`. If either differs from `status.md`, record the
measured one and say so in the task report, because Task 8 corrects from
measurements rather than from the document.

---

## Task 1: O155, a third word for a score of 1

**Files:**
- Modify: `web/src/curation.ts` (`strength`, under `// --- pure helpers`)
- Modify: `web/src/curation.test.ts` (`describe('strength'`)
- Modify: `web/src/ProposalRow.test.tsx` (the case `draws its collapsed line from its own proposal, and asks the server for nothing`)
- Modify: `web/src/Proposals.test.tsx` (the `.score` and `className` assertions in the three-proposal case)
- Modify: `web/src/styles/admin.css` (the comment above `.proposal-list li`)

**Interfaces:**
- Consumes: `INBOX_MIN_SCORE` from `curation.ts`, already exported.
- Produces: `strength(score: number): 'strong' | 'maybe' | 'weak'`.
  `ProposalRow.tsx` writes its answer as the `<li>`'s class and as the
  `.score` chip's text, and needs no change.

- [ ] **Step 1: Write the failing tests**

In `web/src/curation.test.ts`, replace the whole `describe('strength', …)`
block with:

```ts
describe('strength', () => {
  it('calls two signals strong, one maybe, and none weak — O155', () => {
    // The scale is {0,1,2,3}: `edition-of` adds one per signal and the two
    // certain kinds sit at 3. Strong is 2 and above; 1 is one signal, which
    // `proposals.rs` calls "exactly the coin flip" and which has its own
    // word, so that "weak" means exactly what "Show weak proposals" reveals.
    expect(strength(3)).toBe('strong')
    expect(strength(2)).toBe('strong')
    expect(strength(1)).toBe('maybe')
    expect(strength(0)).toBe('weak')
  })

  it('draws the weak line exactly where the inbox threshold does', () => {
    // O155 itself: the two lines lived in two places and disagreed, so 13
    // of the 17 rows the inbox shows by default said "weak" beneath an
    // unticked "Show weak proposals". Pinned to the constant rather than to
    // 1, so moving either line alone is a red test.
    expect(strength(INBOX_MIN_SCORE)).not.toBe('weak')
    expect(strength(INBOX_MIN_SCORE - 1)).toBe('weak')
  })
})
```

In `web/src/ProposalRow.test.tsx`, in the case
`draws its collapsed line from its own proposal, and asks the server for nothing`
(its fixture has `score: 1`), change:

```tsx
    expect(host.querySelector('.score')?.textContent).toBe('weak')
```

to

```tsx
    expect(host.querySelector('.score')?.textContent).toBe('maybe')
```

and change:

```tsx
    expect(host.querySelector('li')?.className).toBe('weak')
```

to

```tsx
    expect(host.querySelector('li')?.className).toBe('maybe')
```

In `web/src/Proposals.test.tsx`, in the case whose fixture has scores 2, 1
and 0, change:

```tsx
    expect(column('.score')).toEqual(['strong', 'weak', 'weak'])
    expect(rows.map((li) => li.className)).toEqual(['strong', 'weak', 'weak'])
```

to

```tsx
    expect(column('.score')).toEqual(['strong', 'maybe', 'weak'])
    expect(rows.map((li) => li.className)).toEqual(['strong', 'maybe', 'weak'])
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default src/curation.test.ts
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default src/ProposalRow.test.tsx src/Proposals.test.tsx
```

Expected: `calls two signals strong, one maybe, and none weak` fails on
`strength(1)` (`'weak'` vs `'maybe'`), and so does
`draws the weak line exactly where the inbox threshold does`. The two jsdom
cases fail on `'weak'` vs `'maybe'`.

- [ ] **Step 3: Implement**

In `web/src/curation.ts`, replace:

```ts
export const strength = (score: number): 'strong' | 'weak' =>
  score >= 2 ? 'strong' : 'weak'
```

with:

```ts
/**
 * The word a proposal's score is shown as, on its chip and its row — O155.
 *
 * **Three words for the scale's three meanings.** `edition-of` adds one per
 * signal — the suffix in the edition vocabulary, an equal volume count — and
 * the two certain kinds sit at 3. So 0 is no signal, which is C3 §4's named
 * false positive and what `INBOX_MIN_SCORE` hides; 1 is one signal, which
 * `proposals.rs` calls "exactly the coin flip"; 2 and up is strong, as the
 * curation build spec set it. Until 2026-09-25 this answered `weak` for 1 as
 * well, so the inbox's default view chipped 13 of its 17 rows `weak`
 * directly beneath an unticked "Show weak proposals". With the third word,
 * "weak" means exactly what that box reveals.
 */
export const strength = (score: number): 'strong' | 'maybe' | 'weak' =>
  score >= 2 ? 'strong' : score >= 1 ? 'maybe' : 'weak'
```

In `web/src/styles/admin.css`, replace the first paragraph of the comment
above `.proposal-list li`:

```css
/* The strength strip, on the edge of the row. `strength()` puts `strong` or
 * `weak` on the `<li>`, and the threshold defaults to 1 — so a weak row on
 * screen is one somebody asked to see. A strong one is marked; a weak one is
 * left plain rather than dimmed, because dimming would take contrast off the
 * sentence that says why the proposal exists.
```

with:

```css
/* The strength strip, on the edge of the row. `strength()` puts `strong`,
 * `maybe` or `weak` on the `<li>`, and the threshold defaults to 1 — so a
 * weak row on screen is one somebody asked to see. (That was not true until
 * O155 gave a score of 1 its own word on 2026-09-25: score-1 rows were on
 * screen by default and said `weak`.) A strong one is marked; the other two
 * are left plain rather than dimmed, because dimming would take contrast off
 * the sentence that says why the proposal exists, and no rule matches
 * `li.maybe` or `li.weak`.
```

Leave the rest of that comment as it is.

- [ ] **Step 4: Run them and watch them pass**

Same two commands as Step 2. Expected: all green.

- [ ] **Step 5: Watch the threshold test catch the defect it names**

Temporarily change `export const INBOX_MIN_SCORE = 1` to `= 2` in
`web/src/curation.ts` and run the `node` command from Step 2. Expected:
`draws the weak line exactly where the inbox threshold does` fails
(`strength(1)` is `'maybe'`, expected `'weak'`). Restore `= 1` and confirm
`git -C /home/davey/projects/taiyaku diff --stat` shows no change to that
line.

- [ ] **Step 6: Run both whole projects, format, lint**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec biome check --write src/curation.ts src/curation.test.ts src/ProposalRow.test.tsx src/Proposals.test.tsx
pnpm -C /home/davey/projects/taiyaku/web run lint
```

Expected: both projects all passed, `node` one more than the baseline
(1363 if the baseline was 1362), `jsdom` unchanged. Lint clean.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add web/src/curation.ts web/src/curation.test.ts web/src/ProposalRow.test.tsx web/src/Proposals.test.tsx web/src/styles/admin.css
git -C /home/davey/projects/taiyaku commit -m "O155: a score of 1 says maybe, so weak is what the box reveals"
```

---

## Task 2: O153, one quiet rule for every `.why` on the Users screen

**Files:**
- Modify: `web/src/styles/admin.css` (the rule `.user-row .why, .user-row .locked` and the comment above it; the comment above `.users .why, .account .why, .backup .what-a-backup-is`)
- Modify: `web/src/styles/admin-config.css.test.ts`

**Interfaces:**
- Consumes: `rules`, `selectors` from `web/src/testing/css.ts`.
- Produces: nothing later tasks use.

- [ ] **Step 1: Write the failing test**

In `web/src/styles/admin-config.css.test.ts`, change the import line
`import { rules, withoutComments } from '../testing/css'` to
`import { rules, selectors, withoutComments } from '../testing/css'`, and
append at the end of the file:

```ts
/**
 * O153: every `.why` on `/config/users` is one kind of sentence, an
 * explanation beneath the control it is about, so one rule quiets them all.
 * It read `.user-row .why` until 2026-09-25, which reached the role hint and
 * the two inside `.reset` and missed the add-account form's, the one `.why`
 * outside a card. Measured in Chromium on 2026-09-24: that one computed
 * 16px in `--text` beside two at 12px in `--muted`, louder than the form's
 * own labels.
 */
describe('admin.css quiets every explanation on the users screen', () => {
  it('reaches .users .why rather than only the ones inside a card', () => {
    // Found by its body rather than its prelude, because the prelude is
    // what this case is about.
    const quiet = rules(adminSource).filter(
      ({ body }) =>
        /(?:^|;)\s*color:\s*var\(--muted\)/.test(body) &&
        /(?:^|;)\s*font-size:\s*var\(--text-meta\)/.test(body),
    )
    expect(
      quiet.filter(({ prelude }) => selectors(prelude).includes('.users .why')),
    ).toHaveLength(1)
    // And the narrow one is gone rather than left beside it, where it would
    // be a second rule for the same three paragraphs to drift apart in.
    expect(
      quiet.some(({ prelude }) => selectors(prelude).includes('.user-row .why')),
    ).toBe(false)
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default src/styles/admin-config.css.test.ts
```

Expected: FAIL. The first assertion finds 0 rules, expected length 1.

- [ ] **Step 3: Implement**

In `web/src/styles/admin.css`, change the rule

```css
.user-row .why,
.user-row .locked {
  color: var(--muted);
  font-size: var(--text-meta);
}
```

to

```css
.users .why,
.user-row .locked {
  color: var(--muted);
  font-size: var(--text-meta);
}
```

and add this paragraph at the end of the comment directly above it (the
one beginning "Descendant, not child"), before its closing `*/`:

```css
   **`.users`, not `.user-row`, since O153 (2026-09-25).** The add-account
   form's `.why` sits outside every card, so `.user-row .why` never reached
   it, and it was the one explanation on this screen drawn at 16px in
   `--text`. `.locked` stays scoped to the card because nothing outside one
   carries it.
```

In the comment above `.users .why, .account .why, .backup .what-a-backup-is`,
replace the sentence

```css
   above this one is a descendant selector, so the two in `.reset` take
   `--text-meta` exactly like the role hint and match it; only the one in
   `.add-user`, which no rule here reaches, is larger. O153 has that.
```

with

```css
   above this one is a descendant selector, so the two in `.reset` take
   `--text-meta` exactly like the role hint and match it. The one in
   `.add-user` was larger until O153 widened that rule to `.users .why`.
```

(If the wrapping differs, match on the words. The sentence ends "O153
has that.")

- [ ] **Step 4: Run it and watch it pass**

Same command as Step 2. Expected: PASS.

- [ ] **Step 5: Watch it catch the defect**

Put `.user-row .why,` back in place of `.users .why,` and run Step 2's
command. Expected: FAIL. Restore `.users .why,`.

- [ ] **Step 6: Run the whole `node` project, format, lint**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec biome check --write src/styles/admin-config.css.test.ts src/styles/admin.css
pnpm -C /home/davey/projects/taiyaku/web run lint
```

Expected: all passed, one more than after Task 1. Lint clean.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add web/src/styles/admin.css web/src/styles/admin-config.css.test.ts
git -C /home/davey/projects/taiyaku commit -m "O153: every explanation on the users screen is quiet"
```

---

## Task 3: O149, the Provenance table shows what is set

**Files:**
- Modify: `web/src/rules.ts` (add after `export interface Preview { … }`)
- Modify: `web/src/rules.test.ts`
- Modify: `web/src/PreviewTabs.tsx` (the import, the local `FIELDS`, the head comment's "seven fields", the Matched files cap notice, the Provenance branch)
- Modify: `web/src/PreviewTabs.test.tsx`
- Modify: `web/src/PreviewTabs.browser.test.tsx` (the fixture's `decided_by`, its doc comment, the eight-column case)
- Modify: `web/src/styles/admin.css` (new `.preview .unset` after `.preview th`; the rule-editor comment's column sentence)

**Interfaces:**
- Consumes: `Row` and `Decided` from `rules.ts`.
- Produces, in `web/src/rules.ts`:
  - `export const FIELDS: readonly ['series', 'edition_key', 'kind', 'language', 'volume', 'chapter', 'title']`
  - `export type Field = (typeof FIELDS)[number]`
  - `export function unsetFields(rows: Row[]): Field[]`
  - `export function unsetSentence(unset: readonly Field[], shown: number, truncated: boolean): string`
    (called only with `unset.length > 0`)

- [ ] **Step 1: Write the failing helper tests**

In `web/src/rules.test.ts`, add `FIELDS`, `type Row`, `unsetFields` and
`unsetSentence` to the `./rules` import (Biome sorts the list in Step 8),
and append:

```ts
/** A row deciding exactly `decided_by`, and nothing else about it varying. */
const deciding = (
  rel_path: string,
  decided_by: Row['decided_by'],
): Row => ({
  rel_path,
  root_id: 1,
  before: null,
  after: null,
  matched: true,
  decided_by,
})

describe('unsetFields — O149', () => {
  it('returns the fields no row decided, in FIELDS order', () => {
    // Two rows deciding *different* fields, so "the fields the first row
    // left unset" and "the fields some row left unset" both give a wrong
    // answer here. Expected in `FIELDS` order, which is not alphabetical
    // (`chapter` would come first), so a sort is caught too.
    expect(
      unsetFields([
        deciding('a.cbz', { title: { kind: 'rule', index: 0 } }),
        deciding('b.cbz', { series: { kind: 'rule', index: 1 } }),
      ]),
    ).toEqual(['edition_key', 'kind', 'language', 'volume', 'chapter'])
  })

  it('counts a root default as decided', () => {
    // "root default" is an answer the table draws. Only a field *nothing*
    // decided is the column of dashes O149 drops.
    expect(
      unsetFields([
        deciding('a.cbz', { edition_key: { kind: 'root_default' } }),
      ]),
    ).not.toContain('edition_key')
  })

  it('keeps FIELDS as the seven rules::Field declares, in its order', () => {
    expect(FIELDS).toEqual([
      'series',
      'edition_key',
      'kind',
      'language',
      'volume',
      'chapter',
      'title',
    ])
  })
})

describe('unsetSentence — O149', () => {
  it('lists several with "and" before the last', () => {
    expect(
      unsetSentence(['edition_key', 'kind', 'language', 'title'], 224, false),
    ).toBe(
      'edition_key, kind, language and title are not set for any of these 224 files.',
    )
  })

  it('says "is" for one field', () => {
    expect(unsetSentence(['title'], 2, false)).toBe(
      'title is not set for any of these 2 files.',
    )
  })

  it('says "this file" for one row', () => {
    expect(unsetSentence(['kind', 'title'], 1, false)).toBe(
      'kind and title are not set for this file.',
    )
  })

  it('says "the first N" when the route capped the rows', () => {
    // The sentence is only true of the rows it was handed, and 500 read as
    // "these 500 files" is easy to take for the whole library.
    expect(unsetSentence(['title'], 500, true)).toBe(
      'title is not set for any of the first 500 files.',
    )
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default src/rules.test.ts
```

Expected: FAIL. `unsetFields`, `unsetSentence` and `FIELDS` are not
exported from `./rules`.

- [ ] **Step 3: Implement the helpers**

In `web/src/rules.ts`, directly after the closing `}` of
`export interface Preview { … }`, add:

```ts
/** The seven fields, in the order `rules::Field` declares them. */
export const FIELDS = [
  'series',
  'edition_key',
  'kind',
  'language',
  'volume',
  'chapter',
  'title',
] as const

export type Field = (typeof FIELDS)[number]

/**
 * The fields no row has a decision for, in `FIELDS` order — the columns
 * O149 drops from the Provenance table.
 *
 * **A field the root default decided counts as set.** "root default" is an
 * answer the table draws; only a field *nothing* decided is the column of
 * dashes this exists to drop. Measured on the real library with rule 1
 * selected, 2026-09-22: `edition_key`, `kind`, `language` and `title` were
 * decided in 0 of 224 rows, and at 390px those four columns were 248px of a
 * 614px table in a 343px box.
 */
export function unsetFields(rows: Row[]): Field[] {
  return FIELDS.filter((field) =>
    rows.every((row) => row.decided_by[field] === undefined),
  )
}

/**
 * The line above the Provenance table naming what `unsetFields` dropped, so
 * "nothing here sets `language`" survives as one sentence rather than as a
 * column of dashes — which is why the user chose this over dropping the
 * columns silently. Called only with at least one field.
 *
 * **"The first N" when the route capped the rows**, because the sentence is
 * true only of the rows it was handed, and "these 500 files" reads as the
 * whole library.
 */
export function unsetSentence(
  unset: readonly Field[],
  shown: number,
  truncated: boolean,
): string {
  const names =
    unset.length === 1
      ? unset[0]
      : `${unset.slice(0, -1).join(', ')} and ${unset[unset.length - 1]}`
  const verb = unset.length === 1 ? 'is' : 'are'
  if (shown === 1 && !truncated) return `${names} ${verb} not set for this file.`
  const scope = truncated ? `the first ${shown} files` : `these ${shown} files`
  return `${names} ${verb} not set for any of ${scope}.`
}
```

- [ ] **Step 4: Run them and watch them pass**

Same command as Step 2. Expected: PASS.

- [ ] **Step 5: Watch the helper tests catch their defects**

Apply each of these by hand, run Step 2's command, see the named case go
red, and restore before the next one:

1. `rows.every(` → `rows.some(`. Expected red:
   `returns the fields no row decided, in FIELDS order`.
2. `row.decided_by[field] === undefined` →
   `row.decided_by[field]?.kind !== 'rule'`. Expected red:
   `counts a root default as decided`.
3. `return FIELDS.filter(…)` → `return [...FIELDS.filter(…)].sort()`.
   Expected red: `returns the fields no row decided, in FIELDS order`.
4. `truncated ? \`the first ${shown} files\`` → always
   `\`these ${shown} files\``. Expected red:
   `says "the first N" when the route capped the rows`.

- [ ] **Step 6: Write the failing component tests**

In `web/src/PreviewTabs.test.tsx`, append inside `describe('PreviewTabs', …)`
(before its closing `})`):

```tsx
  /**
   * O149. The default `row()` decides `series` (rule) and `edition_key`
   * (root default), so it leaves five fields unset. Each case passes
   * distinct `rel_path`s because the rows are keyed on them.
   */
  it('draws only the columns some row decided, a cell for each — O149', () => {
    const { host, click } = render({
      preview: preview({
        rows: [
          row({
            rel_path: 'a.cbz',
            decided_by: { volume: { kind: 'rule', index: 1 } },
          }),
          row({ rel_path: 'b.cbz' }),
        ],
      }),
    })
    click('Provenance')
    const heads = [
      ...host.querySelectorAll('table.provenance thead th'),
    ].map((th) => th.textContent)
    expect(heads).toEqual(['file', 'series', 'edition_key', 'volume'])
    // Per row and per cell, so a body still drawing seven cells under four
    // headings — every column shifted — is red.
    const cells = [...host.querySelectorAll('table.provenance tbody tr')].map(
      (tr) => [...tr.querySelectorAll('td')].map((td) => td.textContent),
    )
    expect(cells).toEqual([
      ['a.cbz', '—', '—', 'rule 2'],
      ['b.cbz', 'rule 1', 'root default', '—'],
    ])
  })

  it('names the dropped columns above the table, not beneath it — O149', () => {
    const { host, click } = render({
      preview: preview({
        rows: [row({ rel_path: 'a.cbz' }), row({ rel_path: 'b.cbz' })],
      }),
    })
    click('Provenance')
    const line = host.querySelector<HTMLElement>('.preview p.unset')
    expect(line?.textContent).toBe(
      'kind, language, volume, chapter and title are not set for any of these 2 files.',
    )
    // Above: beneath 224 rows, which is the real library's case, is a
    // sentence nobody reaches.
    const scroll = host.querySelector('.preview .table-scroll')
    expect(scroll).not.toBeNull()
    expect(
      line!.compareDocumentPosition(scroll!) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()
  })

  it('says nothing, and draws all eight columns, when every field is decided — O149', () => {
    const rule0 = { kind: 'rule', index: 0 } as const
    const { host, click } = render({
      preview: preview({
        rows: [
          row({
            rel_path: 'a.cbz',
            decided_by: {
              series: rule0,
              edition_key: rule0,
              kind: rule0,
              language: rule0,
              volume: rule0,
              chapter: rule0,
              title: rule0,
            },
          }),
        ],
      }),
    })
    click('Provenance')
    expect(host.querySelector('.preview p.unset')).toBeNull()
    expect(host.querySelectorAll('table.provenance thead th')).toHaveLength(8)
  })

  it('says it was capped on the Provenance tab too — O149', () => {
    // `PreviewTabs`' own rule is that the row cap is drawn, and until
    // 2026-09-25 only Matched files drew it.
    const { host, click } = render({
      preview: preview({
        rows: [row({ rel_path: 'a.cbz' }), row({ rel_path: 'b.cbz' })],
        rows_truncated: true,
      }),
    })
    click('Provenance')
    expect(host.querySelector('.preview p.unset')?.textContent).toBe(
      'kind, language, volume, chapter and title are not set for any of the first 2 files.',
    )
    expect(host.querySelector('.preview p.truncated')?.textContent).toBe(
      'The first 500 files only — more matched than fit here.',
    )
  })
```

- [ ] **Step 7: Run them and watch them fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default src/PreviewTabs.test.tsx
```

Expected: the first, second and fourth new cases fail. The first sees
eight headings. The second finds no `p.unset`. The fourth finds neither
paragraph. The third passes today, because eight columns and no line is
the current behaviour for that fixture, and it is watched red in Step 10.

- [ ] **Step 8: Implement the component**

In `web/src/PreviewTabs.tsx`:

(a) Change `import type { Preview, Rule } from './rules'` to
`import { type Field, FIELDS, type Preview, type Rule, unsetFields, unsetSentence } from './rules'`.

(b) Delete the local constant and its comment:

```tsx
/** The seven fields, in the order `rules::Field` declares them. */
const FIELDS = [
  'series',
  'edition_key',
  'kind',
  'language',
  'volume',
  'chapter',
  'title',
] as const
```

(c) In the head comment, change "one down the file list, one across the
seven fields." to "one down the file list, one across the fields some row
decided (O149)."

(d) Directly after `const overrides = preview?.overrides ?? []`, add:

```tsx
  /**
   * O149: the Provenance table draws a column only for a field some row
   * decided, and names the rest in one line above it. With rule 1 selected
   * on the real library, four of the seven were decided in none of 224 rows.
   */
  // Annotated: without it the empty branch types as `never[]`, and
  // `includes` on the union then takes `never`.
  const unset: Field[] =
    preview === undefined ? [] : unsetFields(preview.rows)
  const drawn = FIELDS.filter((field) => !unset.includes(field))
  /** Drawn on both tabs that list files, so neither stops silently at 500. */
  const capped = (
    <p className="truncated">
      The first 500 files only — more matched than fit here.
    </p>
  )
```

(e) In the Matched files branch, replace

```tsx
                {preview.rows_truncated && (
                  <p className="truncated">
                    The first 500 files only — more matched than fit here.
                  </p>
                )}
```

with

```tsx
                {preview.rows_truncated && capped}
```

(f) Replace the whole Provenance branch — from `{tab === 'Provenance' &&`
to the `))}` that closes it — with:

```tsx
          {tab === 'Provenance' &&
            (preview.rows.length === 0 ? (
              <p className="empty">No files to show provenance for.</p>
            ) : (
              <>
                {unset.length > 0 && (
                  <p className="unset">
                    {unsetSentence(
                      unset,
                      preview.rows.length,
                      preview.rows_truncated,
                    )}
                  </p>
                )}
                <div className="table-scroll">
                  <table className="provenance">
                    <thead>
                      <tr>
                        <th>file</th>
                        {drawn.map((field) => (
                          <th key={field}>{field}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {preview.rows.map((row) => (
                        <tr key={`${row.root_id}:${row.rel_path}`}>
                          <td>{row.rel_path}</td>
                          {drawn.map((field) => {
                            const decided = row.decided_by[field]
                            return (
                              <td key={field}>
                                {decided === undefined
                                  ? '—'
                                  : decided.kind === 'root_default'
                                    ? 'root default'
                                    : `rule ${decided.index + 1}`}
                              </td>
                            )
                          })}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {preview.rows_truncated && capped}
              </>
            ))}
```

In `web/src/styles/admin.css`, directly after the `.preview th { … }` rule,
add:

```css
/* O149's line above the Provenance table, naming the fields no row decided
 * and so the columns it dropped. Quiet like every explanation on these
 * screens (`.users .why`): it says why the table is the width it is, and is
 * not part of the table. */
.preview .unset {
  max-width: 60ch;
  margin: 0 0 var(--space-2);
  color: var(--muted);
  font-size: var(--text-meta);
}
```

In the rule-editor comment above `.rule-editor {`, replace

```css
 * width: Provenance is *file plus one column per metadata field*, 8 columns
 * on a library with 4 rules, and it is the one thing here that cannot be
 * given width later. */
```

with

```css
 * width: Provenance is *file plus one column per metadata field some row
 * decided* — up to 8, and 4 on the real library at rule 1 once O149 dropped
 * the fields nothing decides — and it is the one thing here that cannot be
 * given width later. */
```

(match on the words if the wrapping differs).

Then format: `pnpm -C /home/davey/projects/taiyaku/web exec biome check --write src/rules.ts src/rules.test.ts src/PreviewTabs.tsx src/PreviewTabs.test.tsx src/styles/admin.css`.

- [ ] **Step 9: Run them and watch them pass**

Step 2's and Step 7's commands. Expected: all green.

- [ ] **Step 10: Watch the component tests catch their defects**

One at a time, restoring after each:

1. Change both `drawn.map` in the Provenance branch back to `FIELDS.map`.
   Expected red: `draws only the columns some row decided, a cell for each`.
2. Change only the `<tbody>`'s `drawn.map` back to `FIELDS.map`. Expected
   red: the same case, on `cells`.
3. Move the `{unset.length > 0 && (…)}` block from above
   `<div className="table-scroll">` to below `</div>`. Expected red:
   `names the dropped columns above the table, not beneath it`.
4. Change `unset.length > 0 &&` to `true &&`. Expected red:
   `says nothing, and draws all eight columns, when every field is decided`.
   (`unsetSentence([])` then renders " and undefined are not set…", and
   `p.unset` is not null.)
5. Delete `{preview.rows_truncated && capped}` from the Provenance branch.
   Expected red: `says it was capped on the Provenance tab too`.

- [ ] **Step 11: Keep the browser test on the eight-column case**

`web/src/PreviewTabs.browser.test.tsx`'s fixture decides only `series` and
`volume`, so after this task its Provenance table would be three columns and
its case `scrolls the eight-column table inside its own box instead` would
be testing something else under that name. Keep it the widest case.

In the `rows` fixture, replace

```tsx
  decided_by: {
    series: { kind: 'rule', index: 0 },
    volume: { kind: 'rule', index: 1 },
  },
```

with

```tsx
  // All seven, because Provenance draws a column only for a field some row
  // decided (O149), and this file's case is the widest table there is.
  decided_by: {
    series: { kind: 'rule', index: 0 },
    edition_key: { kind: 'root_default' },
    kind: { kind: 'root_default' },
    language: { kind: 'root_default' },
    volume: { kind: 'rule', index: 1 },
    chapter: { kind: 'rule', index: 1 },
    title: { kind: 'rule', index: 0 },
  },
```

In the doc comment above `const rows`, replace

```tsx
 * Real paths, because the widths under test are their widths. `FIELDS` in
 * `PreviewTabs.tsx` is a seven-element constant, so Provenance is 8 columns
 * whatever the data holds; what the rows have to supply is `rel_path`
 * strings as long as a real library's.
```

with

```tsx
 * Real paths, because the widths under test are their widths. Provenance
 * draws `file` plus a column per field some row decided (O149), so these
 * rows decide all seven to keep it the eight-column case; what else they
 * have to supply is `rel_path` strings as long as a real library's.
```

In `scrolls the eight-column table inside its own box instead`, directly
after `await settle()` inside the Provenance block, add:

```tsx
      // The premise, pinned: an edit to the fixture that stops deciding a
      // field makes this a narrower table under the same name.
      expect(host.querySelectorAll('table.provenance thead th'), tab).toHaveLength(8)
```

Run the whole browser project (it needs Chromium; if
`pnpm -C web exec playwright install chromium` has not been run, the gate
reports it as NOT MEASURED, which is not a pass):

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project browser --reporter=default 2>&1 | grep -E '^ +Tests |FAIL'
```

Expected: `Tests  103 passed (103)`. Then delete `title: { kind: 'rule',
index: 0 },` from the fixture, run it again, and see the new
`toHaveLength(8)` go red. Restore it.

- [ ] **Step 12: Run both whole projects, lint**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web run lint
pnpm -C /home/davey/projects/taiyaku/web run typecheck
```

Expected: all passed, `node` up 7 from after Task 2 and `jsdom` up 4 from
the baseline. Lint and typecheck clean.

- [ ] **Step 13: Commit**

```bash
git -C /home/davey/projects/taiyaku add web/src/rules.ts web/src/rules.test.ts web/src/PreviewTabs.tsx web/src/PreviewTabs.test.tsx web/src/PreviewTabs.browser.test.tsx web/src/styles/admin.css
git -C /home/davey/projects/taiyaku commit -m "O149: Provenance draws the fields something decided, and names the rest"
```

---

## Task 4: O159, the card's checkboxes hold without greying

**Files:**
- Modify: `web/src/UserRow.tsx` (the three `.checks` checkboxes and the grant-list checkbox)
- Modify: `web/src/UserRow.test.tsx` (`holds every control until the reload it triggered has landed`; the comment in `deadens the confirm while another write is in flight`)

**Interfaces:**
- Consumes: `begin`, `settled` and `inFlight` in `UserRow.tsx`, unchanged.
- Produces: nothing later tasks use.

- [ ] **Step 1: Write the failing test**

In `web/src/UserRow.test.tsx`, in the case
`holds every control until the reload it triggered has landed`, replace
everything from `const second = control<HTMLInputElement>(host, 'input[name="root-4"]')`
down to and including `expect(usersApi.patchUser).toHaveBeenCalledTimes(1)`
with:

```tsx
    // **Held, but not greyed — O159.** Every checkbox on the card says it is
    // held to a screen reader and keeps its fill. `disabled` took the fill
    // away, so one tick flashed every box on the card.
    const boxes = [
      ...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ]
    expect(boxes.map((box) => box.name)).toEqual([
      'can_download',
      'can_stream',
      'can_stamp',
      'root-3',
      'root-4',
    ])
    for (const box of boxes) {
      expect(box.disabled, box.name).toBe(false)
      expect(box.getAttribute('aria-disabled'), box.name).toBe('true')
    }
    // The rest of the card keeps `disabled`. Its greying is not what O159
    // was about.
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(true)

    // **The lock is the ref, and nothing stands in front of it now.** A
    // tick lands on a live box, reaches `save`, and `begin` refuses it.
    const second = control<HTMLInputElement>(host, 'input[name="root-4"]')
    const stream = control<HTMLInputElement>(host, 'input[name="can_stream"]')
    await act(async () => {
      second.click()
      stream.click()
      await Promise.resolve()
    })
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    // And each box shows what the card holds, not the refused tick.
    expect(second.checked).toBe(false)
    expect(stream.checked).toBe(true)
```

In the same case, change the final assertion

```tsx
    expect(
      control<HTMLInputElement>(host, 'input[name="root-4"]').disabled,
    ).toBe(false)
```

to

```tsx
    expect(
      control<HTMLInputElement>(host, 'input[name="root-4"]').getAttribute(
        'aria-disabled',
      ),
    ).toBe('false')
```

In `deadens the confirm while another write is in flight`, change the
comment's first line from "Every other control on the card carries
`disabled={saving}`; this one" to "Every other control on the card is held
while a write is in flight; this one".

- [ ] **Step 2: Run it and watch it fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default src/UserRow.test.tsx
```

Expected: FAIL on `expect(box.disabled, box.name).toBe(false)` for
`can_download`.

- [ ] **Step 3: Implement**

In `web/src/UserRow.tsx`, on the three `.checks` checkboxes (`can_download`,
`can_stream`, `can_stamp`) and on the grant list's checkbox
(`name={\`root-${root.id}\`}`), change `disabled={saving}` to
`aria-disabled={saving}`. Leave every other `disabled` in the file alone.

Directly above `<div className="checks">`, add:

```tsx
      {/* **`aria-disabled`, not `disabled`, on every checkbox on this card —
          here and in the grant list below. O159.** `disabled` draws a
          checked box in the UA's grey and takes its `accent-color` fill
          with it, so one tick flashed every box on the card. `accent-color`,
          `opacity` and `filter` on `:disabled` did not bring the fill back
          in Chromium on 2026-09-25; only redrawing the box with
          `appearance: none` could, which is a redesign of every checkbox.
          The lock never lived in the attribute: `begin` refuses a second
          write through `inFlight`, and each box is controlled, so React
          puts a refused tick back. The role select and the buttons keep
          `disabled`. */}
```

- [ ] **Step 4: Run it and watch it pass**

Step 2's command. Expected: PASS, the whole file.

If the case fails on `expect(second.checked).toBe(false)`, React did not
put the refused tick back, and the box is showing a grant the card does not
hold. Stop and report it. Do not add `preventDefault` or anything else to
make it pass without saying so, because that is a change to the design.

- [ ] **Step 5: Watch it catch both defects**

One at a time, restoring after each:

1. Put `disabled={saving}` back on the `can_stream` checkbox only. Expected
   red: the `box.disabled` loop, at `can_stream`.
2. In `begin`, delete `if (inFlight.current) return false`. Expected red:
   `toHaveBeenCalledTimes(1)`. **This is the one that matters.** It proves
   the case tests the lock and not just the attribute.

- [ ] **Step 6: Run the whole `jsdom` project, format, lint**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec biome check --write src/UserRow.tsx src/UserRow.test.tsx
pnpm -C /home/davey/projects/taiyaku/web run lint
```

Expected: all passed, the count unchanged from after Task 3 (this task
changes a case and adds none). Lint clean.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add web/src/UserRow.tsx web/src/UserRow.test.tsx
git -C /home/davey/projects/taiyaku commit -m "O159: a user card's checkboxes hold through a save without greying"
```

---

## Task 5: O157, a 44px label for the rule-enable box

**Files:**
- Modify: `web/src/RuleList.tsx` (the enable `<input type="checkbox">` inside `.rule-row`)
- Modify: `web/src/RuleList.test.tsx`
- Modify: `web/src/styles/admin.css` (the last `@media (pointer: coarse)` block: a new rule, and the paragraph beginning "**`.rule-list`'s enable box gets the 24 and nothing else**")
- Modify: `web/src/styles/admin-proposals.css.test.ts` (inside `describe('admin.css carries a coarse-pointer floor'`)

**Interfaces:**
- Consumes: the `coarse` and `inRule` helpers already defined inside
  `describe('admin.css carries a coarse-pointer floor', …)`.
- Produces: `.rule-row label.enable`, which Task 7 measures.

- [ ] **Step 1: Write the failing tests**

In `web/src/RuleList.test.tsx`, after the case `toggles enable from the row`,
add:

```tsx
  it('wraps the enable box in a wordless label of its own — O157', () => {
    // The label is what `admin.css`'s coarse block grows to 44×44. It adds a
    // target, not a name: the box is still named by its own `aria-label`.
    const { host, onToggle } = render()
    const boxes = [
      ...host.querySelectorAll<HTMLInputElement>(
        '.rule-row input[type="checkbox"]',
      ),
    ]
    expect(boxes.length).toBeGreaterThan(1)
    for (const [i, box] of boxes.entries()) {
      const label = box.parentElement
      expect(label?.tagName, `row ${i + 1}`).toBe('LABEL')
      expect(label?.className, `row ${i + 1}`).toBe('enable')
      expect(label?.textContent, `row ${i + 1}`).toBe('')
      expect(box.getAttribute('aria-label')).toBe(`rule ${i + 1} enabled`)
    }
    // And a press on the label, outside the box, is a press on the box.
    act(() => {
      host
        .querySelectorAll<HTMLLabelElement>('.rule-row label.enable')[1]
        .click()
    })
    expect(onToggle).toHaveBeenCalledWith(1)
  })
```

In `web/src/styles/admin-proposals.css.test.ts`, inside
`describe('admin.css carries a coarse-pointer floor', …)`, after the case
`gives every subject of the first rule the 44px row itself`, add:

```ts
  /**
   * O157: `RuleList`'s enable box has no row to take the 44, because it has
   * no visible label, so it gets a wordless `<label>` of its own and this
   * rule sizes the label. The box inside keeps the 24px the checkbox rule
   * gives it. 44px on the box itself would draw the button-shaped checkbox
   * this block refuses.
   */
  it('gives the rule-enable box a 44px label of its own', () => {
    for (const declaration of [
      'min-inline-size:\\s*44px',
      'min-block-size:\\s*44px',
      'place-items:\\s*center',
    ]) {
      expect(coarse).toMatch(inRule('.rule-row .enable', declaration))
    }
  })
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default src/RuleList.test.tsx
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default src/styles/admin-proposals.css.test.ts
```

Expected: the RuleList case fails (`'LI'` vs `'LABEL'`, since the box's
parent is the row). The CSS case fails (no `.rule-row .enable`).

- [ ] **Step 3: Implement**

In `web/src/RuleList.tsx`, replace

```tsx
            <input
              type="checkbox"
              checked={rule.enabled}
              disabled={frozen}
              aria-label={`rule ${index + 1} enabled`}
              onChange={() => onToggle(index)}
            />
```

with

```tsx
            {/* **A label of its own, with no words — O157.** The box has no
                visible label, only its `aria-label`, so under a coarse
                pointer the 24px box was the whole tap target. `admin.css`
                grows this label to 44×44 and leaves the box drawn at its
                own size; the `aria-label` stays the box's name. */}
            <label className="enable">
              <input
                type="checkbox"
                checked={rule.enabled}
                disabled={frozen}
                aria-label={`rule ${index + 1} enabled`}
                onChange={() => onToggle(index)}
              />
            </label>
```

In `web/src/styles/admin.css`, inside the **last** `@media (pointer: coarse)`
block, directly before the rule
`:where(.proposals, .account, .config) input[type="checkbox"] {`, add:

```css
  .rule-row .enable {
    display: inline-grid;
    place-items: center;
    min-inline-size: 44px;
    min-block-size: 44px;
  }
```

In the comment above that block, replace the paragraph that begins
"**`.rule-list`'s enable box gets the 24 and nothing else, and that is a
gap rather than an answer.**" and ends "Recorded here so the next reader
sees it was left open rather than missed." with:

```css
   **`.rule-list`'s enable box gets the 24, and its label gets the 44 —
   O157, 2026-09-25.** `RuleList` draws the box with an `aria-label` and no
   visible label, so there was no row to enlarge and the box was the whole
   tap target. It now sits in a wordless `<label className="enable">`, and
   `.rule-row .enable` below makes that label 44×44 with the box centred in
   it. Enlarging the box itself would draw the button-shaped checkbox the
   paragraph above refuses.
```

- [ ] **Step 4: Run them and watch them pass**

Step 2's commands. Expected: PASS.

- [ ] **Step 5: Watch them catch their defects**

One at a time, restoring after each:

1. Unwrap the `<input>` from the `<label>` in `RuleList.tsx`. Expected red:
   the RuleList case.
2. Change `min-block-size: 44px` to `min-block-size: 24px` in
   `.rule-row .enable`. Expected red: the CSS case.

- [ ] **Step 6: Run both whole projects, format, lint**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec biome check --write src/RuleList.tsx src/RuleList.test.tsx src/styles/admin.css src/styles/admin-proposals.css.test.ts
pnpm -C /home/davey/projects/taiyaku/web run lint
```

Expected: all passed, each project one more than after Task 4. Lint clean.
The RuleList suite's other cases, including the O96 `gestures` reader that
finds the box with `.rule-row input[type="checkbox"]`, still pass, because
a descendant selector reaches through the label.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add web/src/RuleList.tsx web/src/RuleList.test.tsx web/src/styles/admin.css web/src/styles/admin-proposals.css.test.ts
git -C /home/davey/projects/taiyaku commit -m "O157: the rule-enable box gets a 44px label"
```

---

## Task 6: O157, the three underlined rows reach 44px to the touch

**Files:**
- Modify: `web/src/styles/admin.css` (the last `@media (pointer: coarse)` block: two new rules, and the comment above the floor's first rule)
- Modify: `web/src/styles/admin-proposals.css.test.ts` (inside the same describe as Task 5)

**Interfaces:**
- Consumes: `coarse` and `inRule`, as in Task 5.
- Produces: `::after` targets on `.config-nav a`, `.proposal-tabs > button`
  and `.preview-tabs > button`, which Task 7 measures.

- [ ] **Step 1: Write the failing tests**

In `web/src/styles/admin-proposals.css.test.ts`, directly after Task 5's
case, add:

```ts
  /**
   * O157: the three underlined one-of-N rows take their 44px from a box
   * with no paint centred on each item, not from their own height, which
   * the floor's `:not()` keeps off them on §12.8's measurement. Both halves
   * are asserted. The `::after` grows the target, and `position: relative`
   * anchors it to its own item. Without that, every tab's target would be
   * placed against the nearest positioned ancestor, one box for the whole
   * row.
   */
  for (const item of [
    '.config-nav a',
    '.proposal-tabs > button',
    '.preview-tabs > button',
  ]) {
    it(`grows ${item} to 44px to the touch without drawing it taller`, () => {
      expect(coarse).toMatch(inRule(item, 'position:\\s*relative'))
      // Every declaration the box needs to land centred on its item: the
      // two insets and the translate together are the centring, and a box
      // missing either inset sits at its static position shifted up and
      // left by half its own size.
      for (const declaration of [
        'content:\\s*""',
        'position:\\s*absolute',
        'inset-block-start:\\s*50%',
        'inset-inline-start:\\s*50%',
        'inline-size:\\s*max\\(100%,\\s*44px\\)',
        'block-size:\\s*max\\(100%,\\s*44px\\)',
        'translate:\\s*-50%\\s+-50%',
      ]) {
        expect(coarse).toMatch(inRule(`${item}::after`, declaration))
      }
      // The item's own box is left alone. A height here is the redesign
      // §12.8 measured and refused.
      expect(coarse).not.toMatch(inRule(item, 'min-height'))
      expect(coarse).not.toMatch(inRule(item, 'min-block-size'))
    })
  }
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default src/styles/admin-proposals.css.test.ts
```

Expected: three new cases fail on `position:\s*relative`.

- [ ] **Step 3: Implement**

In `web/src/styles/admin.css`, inside the last `@media (pointer: coarse)`
block, directly before Task 5's `.rule-row .enable` rule, add:

```css
  /* **O157: the three underlined one-of-N rows reach 44px to the touch
     without being drawn at 44.** The floor's first rule excludes them on a
     measurement: a 44px box puts the underline 12.5px below the word and
     takes the strip from 29px to 49. So the target grows here instead, as
     a box with no paint centred on each item. A press on a pseudo-element
     is a press on its element, so nothing else needs to know. Nothing it
     covers is drawn differently. What it must not cover is a neighbour's
     own target, and that was measured in a touch context at 390px
     (`docs/notes.md`, O157) because no test here can match
     `(pointer: coarse)`. */
  .config-nav a,
  .proposal-tabs > button,
  .preview-tabs > button {
    position: relative;
  }

  .config-nav a::after,
  .proposal-tabs > button::after,
  .preview-tabs > button::after {
    content: "";
    position: absolute;
    inset-block-start: 50%;
    inset-inline-start: 50%;
    inline-size: max(100%, 44px);
    block-size: max(100%, 44px);
    translate: -50% -50%;
  }
```

In the comment above the floor's first rule (the paragraph beginning
"Every button on the three roots **except the two underlined tab rows**."),
add after the sentence ending "a treatment the spec calls a redesign."
(match on the words if the wrapping differs). *(This anchor read "on the
strength of the element name alone" until Task 6's implementer found it
nowhere in `admin.css`: that is §12.8's wording, not the stylesheet's.)*:

```css
     They reach 44px to the touch through the `::after` further down this
     block instead (O157), which grows the target and not the box.
```

- [ ] **Step 4: Run them and watch them pass**

Step 2's command. Expected: PASS.

- [ ] **Step 5: Watch them catch their defects**

One at a time, restoring after each:

1. Delete the `position: relative` rule. Expected red: all three cases.
2. Change `.preview-tabs > button::after` to `.preview-tabs button::after`
   (descendant) in the `::after` prelude. Expected red: the
   `.preview-tabs > button` case, because `inRule` anchors the exact
   selector.
3. Add `min-height: 44px;` to the `position: relative` rule. Expected red:
   all three, on `not.toMatch`.
4. Delete `inset-inline-start: 50%;` from the `::after` rule. Expected red:
   all three.

- [ ] **Step 6: Check the existing tab assertions still hold**

These pin `.proposal-tabs button` and `.preview-tabs button` (descendant,
no `>`) to exactly one rule. The new preludes use `>` and must not trip them:

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests |FAIL'
pnpm -C /home/davey/projects/taiyaku/web exec biome check --write src/styles/admin.css src/styles/admin-proposals.css.test.ts
pnpm -C /home/davey/projects/taiyaku/web run lint
```

Expected: all passed, three more than after Task 5. No FAIL line. Lint
clean.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add web/src/styles/admin.css web/src/styles/admin-proposals.css.test.ts
git -C /home/davey/projects/taiyaku commit -m "O157: the underlined rows reach 44px to the touch, drawn as before"
```

---

## Task 7: The browser pass, and O157's touch measurement

**Files:**
- Modify: `docs/notes.md` (a new section at the end)
- Scratch only: `<scratchpad>/o157-measure.mjs`, screenshots under `target/o-bundle/pass/`

**Interfaces:**
- Consumes: Tasks 1–6 on the branch.
- Produces: the measurement Task 8's O157 close cites, and the screenshots
  the whole-branch review reads.

- [ ] **Step 1: Build the binary and start the two servers**

Rebuild only if `git diff main...HEAD -- crates/` is non-empty (it should be
empty, since this branch touches no Rust):

```bash
git -C /home/davey/projects/taiyaku diff --stat main...HEAD -- crates/
ss -ltnH 'sport = :8093' ; ss -ltnH 'sport = :5293'
```

If nothing listens on 8093, start it in a background task (the Bash tool's
`run_in_background`, not `&`):

```bash
cd /home/davey/projects/taiyaku && exec ./target/release/taiyaku serve --db target/o-bundle/lib/library.db --addr 127.0.0.1:8093 --cache target/o-bundle/cache --insecure-cookies
```

If nothing listens on 5293, start vite the same way:

```bash
cd /home/davey/projects/taiyaku/web && TAIYAKU_SERVE=http://127.0.0.1:8093 exec pnpm exec vite --host 127.0.0.1 --port 5293 --strictPort
```

A vite already running on 5293 serves the branch's current files through
HMR, so it does not need restarting.

- [ ] **Step 2: Write the measurement script to the scratchpad**

`<scratchpad>/o157-measure.mjs`:

```js
// O157's touch measurement. A Chromium context with hasTouch and isMobile
// matches (pointer: coarse), which vitest's browser project does not.
import { createRequire } from 'node:module'

const require = createRequire('/home/davey/projects/taiyaku/web/package.json')
const { chromium } = require('playwright')

const BASE = 'http://127.0.0.1:5293'
const browser = await chromium.launch()
const context = await browser.newContext({
  viewport: { width: 390, height: 844 },
  hasTouch: true,
  isMobile: true,
})
const page = await context.newPage()
await page.goto(`${BASE}/`)
const login = await page.evaluate(() =>
  fetch('/api/session', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: 'local', password: 'drawpw' }),
  }).then((r) => r.status),
)
if (login !== 204) throw new Error(`sign-in answered ${login}`)

/**
 * For each item matching `sel`: its drawn box, and whether a point 1px
 * inside each edge of the grown 44px target lands on it. Then, for every
 * other control on the page, whether its own centre still lands on itself
 * rather than on a grown target.
 */
async function probe(sel) {
  return page.evaluate((sel) => {
    const coarse = matchMedia('(pointer: coarse)').matches
    const items = [...document.querySelectorAll(sel)]
    const out = []
    for (const item of items) {
      item.scrollIntoView({ block: 'center' })
      const r = item.getBoundingClientRect()
      const w = Math.max(r.width, 44)
      const h = Math.max(r.height, 44)
      const cx = r.left + r.width / 2
      const cy = r.top + r.height / 2
      const edges = {
        left: [cx - w / 2 + 1, cy],
        right: [cx + w / 2 - 1, cy],
        top: [cx, cy - h / 2 + 1],
        bottom: [cx, cy + h / 2 - 1],
      }
      const reached = Object.fromEntries(
        Object.entries(edges).map(([edge, [x, y]]) => [
          edge,
          document.elementFromPoint(x, y)?.closest(sel) === item,
        ]),
      )
      out.push({
        item: item.textContent.trim().slice(0, 24),
        drawn: [+r.width.toFixed(1), +r.height.toFixed(1)],
        reached,
      })
    }
    const stolen = []
    for (const c of document.querySelectorAll(
      'main a, main button, main input, main select, main textarea, main label, main summary',
    )) {
      if (c.matches(sel) || c.closest(sel)) continue
      const r = c.getBoundingClientRect()
      if (r.width === 0 || r.height === 0) continue
      c.scrollIntoView({ block: 'center' })
      const q = c.getBoundingClientRect()
      const hit = document.elementFromPoint(q.left + q.width / 2, q.top + q.height / 2)
      if (hit?.closest(sel) && !c.contains(hit)) stolen.push(c.outerHTML.slice(0, 100))
    }
    const doc = document.documentElement
    return { coarse, items: out, stolen, sideways: doc.scrollWidth - doc.clientWidth }
  }, sel)
}

const report = {}
await page.goto(`${BASE}/config/rules`)
await page.waitForSelector('.rule-row')
report['config-nav on /config/rules'] = await probe('.config-nav a')
report['preview-tabs'] = await probe('.preview-tabs > button')
report['rule-enable label'] = await page.evaluate(() =>
  [...document.querySelectorAll('.rule-row label.enable')].map((l) => {
    const r = l.getBoundingClientRect()
    return [+r.width.toFixed(1), +r.height.toFixed(1)]
  }),
)
report['rules sideways'] = await page.evaluate(
  () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
)
await page.goto(`${BASE}/proposals`)
await page.waitForSelector('.proposal-list li')
report['proposal-tabs'] = await probe('.proposal-tabs > button')
await page.goto(`${BASE}/config/users`)
await page.waitForSelector('.user-row')
report['config-nav on /config/users'] = await probe('.config-nav a')
console.log(JSON.stringify(report, null, 2))
await browser.close()
```

- [ ] **Step 3: Run it**

```bash
node <scratchpad>/o157-measure.mjs | tee <scratchpad>/o157-measure.json
```

Expected, for the task to pass:
- `coarse` is `true` in every probe. If it is `false`, the context did not
  match and nothing else in the report means anything.
- every `reached` edge is `true` for every item, so each target reaches 44
  on both axes;
- every `stolen` list is `[]`;
- every `sideways` is `0`, as is `rules sideways`;
- every `rule-enable label` pair is at least `[44, 44]`.

**If a `stolen` list is not empty**, a grown target covers a neighbour's
centre. Do not widen the exclusion or shrink the target by guesswork.
Report the neighbour and the item, and stop. The spec (§2.4) says the axis
that collides is shrunk until it doesn't and the shortfall is recorded, and
choosing that figure belongs in the task report for review.

**If a `reached` edge is `false`**, look at what `elementFromPoint`
returned before concluding anything. A sticky header over the point is a
measurement fault, not a defect.

- [ ] **Step 4: Screenshot each changed screen**

Using the Playwright MCP tools or a copy of the script above, with
`page.addInitScript(() => localStorage.setItem('taiyaku.prefs.theme', JSON.stringify({ choice: 'ink' })))`
for Ink and `{ choice: 'paper' }` for Paper, save to
`target/o-bundle/pass/` at 390×844 (touch context) and 1440×900 (a plain
context), in both themes:

1. `/config/rules`, rule 1 selected, Provenance tab: the line above the
   table, four columns. At 390, record `table.provenance`'s
   `getBoundingClientRect().width` and its `.table-scroll`'s `clientWidth`.
   Task 8's O149 close cites them, and the drawing measured 366 in 343.
2. `/config/users`: all three `.why` paragraphs muted at 12px.
3. `/proposals`: 4 `strong`, 13 `maybe`. Then with "Show weak proposals"
   ticked: 18 more reading `weak`.
4. `/config/users` at 390 in the touch context, mid-save: slow the PATCH
   with `page.route('**/api/library/users/*', async (r) => { await new Promise((f) => setTimeout(f, 1500)); await r.continue() })`,
   tick "May stream pages", and screenshot within the 1.5 s. The checked
   boxes keep their red fill.

Look at every screenshot before moving on. Confirm on the pixels what the
step says each shows, e.g. the chips' words and the line's position.

- [ ] **Step 5: Record the measurement in `docs/notes.md`**

Append a section at the end of `docs/notes.md`:

```markdown
## O157: the underlined rows and the rule-enable box, to the touch (2026-09-25)

Measured on branch `admin-follow-ups` in a Chromium context with `hasTouch`
and `isMobile` at 390×844, which matches `(pointer: coarse)` (checked in the
run: `matchMedia('(pointer: coarse)').matches` was `true` on every screen),
against a copy of the real library. The vitest `browser` project does not
match it, which is why this is a note and not a test.

<the table of items: screen, item, drawn width×height, and whether each of
the four 44px edges landed on the item — from o157-measure.json, every
item, not a sample>

Stolen neighbours: <[] on every screen, or the list>. Sideways scroll:
<0 on every screen>. Rule-enable labels: <each width×height>.

What this does not show: that a finger lands where `elementFromPoint` says.
The hit test is the browser's own, so it is the same test a tap uses, but
the device pass is where a finger confirms it.
```

Fill each `<…>` from `o157-measure.json`. Write every figure the run
produced and nothing it didn't.

- [ ] **Step 6: Prove the real library was not touched**

```bash
sha256sum /home/davey/temp/manga-test/library.db
find /home/davey/temp/manga-test -newermt '2026-09-25 00:00' | wc -l
git -C /home/davey/projects/taiyaku status --short
```

Expected: the checksum from Global Constraints, `0`, and only
`docs/notes.md` modified.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add docs/notes.md
git -C /home/davey/projects/taiyaku commit -m "O157: the touch measurement"
```

---

## Task 8: `docs/status.md`

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/superpowers/specs/2026-09-17-style-pass-design.md` (§12.8, the O149 bullet)

**Interfaces:**
- Consumes: Tasks 1–7, and the counts measured in Step 1.
- Produces: the document the PR body quotes.

- [ ] **Step 1: Measure, and run the sweep on the way in**

```bash
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project node --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom --reporter=default 2>&1 | grep -E '^ +Tests '
pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project browser --reporter=default 2>&1 | grep -E '^ +Tests '
cd /home/davey/projects/taiyaku
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -n 'One hundred and' docs/status.md | head -3
grep -n -A1 '^What remains:' docs/status.md
```

Expected before editing: 159 rows and 94 closed. **The head's count word
reads "One hundred and fifty-four" against 159 rows**, which is the drift
the ritual warns about, already present on `main`. It is corrected in
Step 4. Record the three test counts. Expected: `node` 1362 + 13 = 1375,
`jsdom` 1121 + 5 = 1126, `browser` 103. If they differ, the measurement
wins. Find out why before writing it down.

- [ ] **Step 2: Close the five rows**

For each of O149, O153, O155, O157 and O159: change `| [ ] |` to `| [x] |`,
and put a bold closing sentence at the very start of the row's What cell,
the way O150's row does (`**Closed 2026-09-25 on …**`), then the row's
existing text left as written. The five closing sentences:

- **O149:** `**Closed 2026-09-25 on the admin follow-ups branch, as the option the user chose against drawings on the real library that day.** \`unsetFields\` and \`unsetSentence\` in \`rules.ts\` drop a field no shown row decided and name it in one quiet line above the table, and the Provenance tab now draws the 500-row cap notice Matched files always did. At 390px the table went from 614px to <Task 7's measured width>px in its <measured clientWidth>px box. **The line sits above the table, not beneath it as §12.8 of the style pass spec first recorded**, because beneath is 224 rows down.`
- **O153:** `**Closed 2026-09-25 on the admin follow-ups branch, with option B chosen against drawings that day:** the quiet-text rule's \`.user-row .why\` became \`.users .why\`, so the add-account hint is 12px in \`--muted\` like the other two.`
- **O155:** `**Closed 2026-09-25 on the admin follow-ups branch, with option C chosen against drawings that day:** \`strength\` answers \`maybe\` for a score of 1, so the default view reads 4 \`strong\` and 13 \`maybe\`, and "Show weak proposals" reveals exactly the 18 rows that say \`weak\`. It keeps both closed decisions of the curation build spec, strong at 2 and the threshold at 1.`
- **O157:** `**Closed 2026-09-25 on the admin follow-ups branch.** The three underlined rows take a 44px target from an \`::after\` with no paint under \`(pointer: coarse)\`, drawn exactly as before, and the rule-enable box sits in a wordless 44×44 \`<label className="enable">\`. Measured in a touch context at 390px, \`docs/notes.md\` "O157". **\`RuleEditor\`'s file input leaves the row rather than closing with it**: it has the \`hidden\` attribute, so it is never drawn and cannot be touched, and the Import button that opens it already clears 44.`
- **O159:** `**Closed 2026-09-25 on the admin follow-ups branch.** The card's checkboxes carry \`aria-disabled\` rather than \`disabled\` while saving, so a checked box keeps its fill. In Chromium, \`accent-color\`, \`opacity\` and \`filter\` on \`:disabled\` did not bring the fill back to a \`disabled\` one. The hold is unchanged, because \`begin\`'s \`inFlight\` ref was always what refused a second write, and \`UserRow.test.tsx\` now proves it by clicking a live box mid-save. The role select and the buttons keep \`disabled\`.`

Update each row's Where cell where a symbol it names moved:
O149's `PreviewTabs.tsx`'s `FIELDS` becomes `rules.ts`'s `FIELDS`,
`unsetFields`, and `PreviewTabs.tsx`'s `provenance` table.

Check each edited row still has exactly six unescaped `|`:

```bash
grep -nE '^\| O1(49|53|55|57|59) ' docs/status.md | while IFS= read -r l; do n=$(printf '%s' "$l" | sed 's/\\|//g' | tr -cd '|' | wc -c); echo "$n ${l:0:12}"; done
```

Expected: `6` on every line.

- [ ] **Step 3: Open O160**

After the O159 row, add:

```markdown
| O160 | [ ] | **The add-account form's explanation speaks the repository's language to an administrator.** `Users.tsx`'s add-account `.why` reads "C4 §4: there is no self-service registration — an administrator makes accounts. A new account is a reader unless this says otherwise, which is the column default `004_auth.sql` chose because any insert that omitted the role minted an administrator." "C4 §4" is a spec section and `004_auth.sql` a migration file, and the person reading the screen can open neither. **Noticed while drawing O153 on 2026-09-25.** O153 made the paragraph quiet and left its words alone, because what a screen *says* is reserved by spec 2026-09-17 §6. **What would settle it**: a rewrite in the reader's terms (who makes accounts, and that a new one is a reader unless the role says otherwise), with the history kept in the source comment, where it is true | `Users.tsx`'s add-account form | the admin follow-ups' design, 2026-09-25 |
```

- [ ] **Step 4: The counts**

1. **Test counts** (the "Six crates, 2237 tests, plus 2,483 in `web/`'s
   TypeScript — 1,362 in the `node` project and 1,121 in the `jsdom` one —
   and a hundred and three more in a **browser** project" sentence). Rust
   stays 2237, since this branch touches no Rust. Write the `node` and
   `jsdom` figures from Step 1 and their sum, and the browser figure in
   words.
2. **The head's count word**: "One hundred and fifty-four" becomes "One
   hundred and sixty", for 160 rows.
3. **The closed sentence**, "**Ninety-four are now closed — O1, … — leaving
   sixty-five.**" It is generated from the table, not edited:

   ```bash
   grep -oE '^\| O[0-9]+ \| \[x\]' docs/status.md | grep -oE 'O[0-9]+' | paste -sd, - | sed 's/,/, /g'
   ```

   Rewrite the list from that output, with "and" before the last, as
   "**Ninety-nine are now closed — … — leaving sixty-one.**". Leave the
   italic history notes after it as they are.
4. **"What remains: the sixty-five open items"** becomes "sixty-one". The
   word wraps across a line break, so edit it where `grep -A1` showed it.

- [ ] **Step 5: §12.8's pointer**

In `docs/superpowers/specs/2026-09-17-style-pass-design.md`, at the end of
the §12.8 bullet that begins "- **O149**, the Provenance table's four
always-empty columns.", add one sentence:
`*Built on 2026-09-25 with the line above the table rather than beneath it — docs/superpowers/specs/2026-09-25-admin-follow-ups-design.md §1.*`

- [ ] **Step 6: Run the sweep on the way out, and read for the O-numbers**

```bash
cd /home/davey/projects/taiyaku
grep -c '^| O[0-9]' docs/status.md                 # expect 160
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # expect 99
grep -n -A1 '^What remains:' docs/status.md
grep -rn -E '\bO(149|153|155|157|159)\b' docs/ web/src/ | grep -v '^docs/status.md:1[0-9]\{3\}:| O'
```

Expected: 160 and 99, "sixty-one", and the last grep's hits all read as
true of the code after this branch. Any present-tense sentence that still
calls one of the five open (like "O153 has that") is corrected here.
Historical narrative stays as written, marked
`*O1xx has since closed — see its row.*` per the ritual.

Render the six edited or added rows through the forge's GFM endpoint to
check nothing leaked across a cell. The token is in
`~/.config/tea/config.yml`, and the ritual memory has the recipe: put the
table's header and separator rows above them, because a fragment without a
header renders nothing, and that reads like a pass.

- [ ] **Step 7: Commit**

```bash
git -C /home/davey/projects/taiyaku add docs/status.md docs/superpowers/specs/2026-09-17-style-pass-design.md
git -C /home/davey/projects/taiyaku commit -m "status: close O149, O153, O155, O157 and O159; open O160"
```

---

## Task 9: The gate

**Files:** none.

- [ ] **Step 1: Run the gate, on its own**

```bash
cd /home/davey/projects/taiyaku && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh
```

Run it as a background task and read its output file when it finishes.
Expected: the last line is exactly `all green`. `all green, except NOT
MEASURED:` is not a pass. Quote the line in the task report.

- [ ] **Step 2: If it is red**

Read the first failure. A `status-test-count` failure means Task 8's Rust
figure is wrong, because that check reads the JUnit report. Fix what it
names, commit the fix as its own commit, and run Step 1 again. Never
`--no-verify`, and never re-run hoping for a different result.

---

## After the tasks

These are not tasks for an implementer. They are what the style pass
spec's §5 list requires before the PR opens:

1. **A whole-branch review** of `main...admin-follow-ups`, and a review of
   each fix round until one finds nothing. After fixing a finding, grep for
   its shape across the branch, not just its function.
2. **The pre-merge claim check** over the PR body, `docs/status.md` and
   `docs/notes.md`: every figure and every present-tense claim, checked
   against the code and the measurement. Run it again, separately, after
   its own fixes.
3. **The PR** on the Gitea forge, through the MCP, not `gh`. The body is the
   squash commit message, so never hard-wrap it. Link the drawings artifact.

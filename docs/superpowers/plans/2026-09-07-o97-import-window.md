# O97 — one writer at a time: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Save and Import mutually exclusive on the rule editor, so neither
can write the rule table while the other is in flight and no gesture made
inside either window is silently discarded.

**Architecture:** The four flags that gate a save today (`savingNow` ref,
`saving` state, and the two conditions reading them) collapse into one
three-valued `writer` — `'save' | 'import' | undefined` — held in a ref for the
synchronous gate and in state for the render. Every frozen surface reads one
derived boolean. A gesture refused or discarded because a write was in flight
says so in a notice slot that the import's own answer cannot wipe.

**Tech Stack:** React 18 + TypeScript, Vitest (`jsdom` project), Biome.

The design this implements is
[`../specs/2026-09-07-o97-import-window-design.md`](../specs/2026-09-07-o97-import-window-design.md).
Read it first; this plan does not restate its reasoning, only its consequences.

## Global Constraints

- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.** Never run it
  with `TAIYAKU_CORPUS` set — the script exports it into an unfiltered
  `cargo test --workspace`, which runs `geometry_experiment` in the debug
  profile (40+ min). Verify the exit by quoting its own `all green` line;
  piping through `tail` or appending `; echo` masks a failure.
- **`tsc` is part of that gate and vitest is not a substitute.** Renaming a
  required prop leaves every existing mount site uncompilable, and O96 shipped
  a file whose 21 tests all passed against a call that would not build.
- **Every test is hand-applied against its own defect and watched to fail
  alone.** A test that passes against the defect it names is this
  repository's most-repeated finding; reading it is not verifying it.
- **Assert per element, never a whole-page `toContain`.** A page-level string
  match cannot tell a disabled Save from a disabled Import.
- **`async`/`await` throughout, never `act(...).then(...)`.** A rejection
  chained off `act`'s thenable is swallowed: an O96 test printed its own
  `AssertionError` and vitest still reported it passing.
- Component tests live in the **`jsdom`** vitest project. Single-file run:
  `cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx`.
- The jsdom project prints `The current testing environment is not configured
  to support act(...)` on every run. That is **O75**, open and pre-existing.
  It is not this branch's, and it is not to be fixed here.
- No attribution lines in commit messages or the PR body.
- Branch is `o97-import-window`, already created, with the design doc
  committed at `e425a3c`.

## File Structure

| File | Responsibility | Change |
| --- | --- | --- |
| `web/src/RuleEditor.tsx` | Owns the gate, both writers, and every notice slot | Modify |
| `web/src/RuleList.tsx` | Draws the list; freezes on one prop | Rename prop |
| `web/src/PreviewTabs.tsx` | Draws the preview; two override options freeze | Rename prop |
| `web/src/RuleEditor.test.tsx` | The window's behaviour | Add 7 tests, 2 fixtures |
| `web/src/RuleList.test.tsx` | 5 mount sites pass the renamed prop | Modify |
| `web/src/PreviewTabs.test.tsx` | 1 mount site passes the renamed prop | Modify |
| `docs/status.md` | O97's row and the five-part close | Modify |
| `docs/notes.md` | The import window's measured length, the drive | Modify |

---

### Task 1: The gate — four flags become one `writer`

A pure refactor. No behaviour changes, no test changes: the existing suite is
the test, and it must pass before and after with nothing edited.

**Files:**
- Modify: `web/src/RuleEditor.tsx:179,185,261-263,270-271,323-324,373,515`

**Interfaces:**
- Consumes: nothing.
- Produces: module-scope `type Writer = 'save' | 'import' | undefined`; inside
  `RuleEditor`, `writer: React.RefObject<Writer>`, `writing: Writer`,
  `setWriting: (w: Writer) => void`, and one derived boolean `saving`
  (`writing === 'save'`). Tasks 2, 3 and 4 all read these. **`frozen` is
  deliberately NOT declared here** — it has no reader until Task 2, and this
  repo sets `noUnusedLocals: true`, so declaring it early fails `tsc -b` and
  invites a suppression directive into shipped code. Task 2 declares it.

- [ ] **Step 1: Run the existing suite and record it green**

```bash
cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx
```

Expected: all pass. Write the number down — it is the figure Step 4 compares
against, and "it still passes" is not checkable without it.

- [ ] **Step 2: Add the type at module scope**

In `web/src/RuleEditor.tsx`, above the `RuleEditor` component:

```ts
/**
 * Which write is in flight, if any. Both writers on this screen — Save and
 * Import — take this gate, and neither runs while the other holds it.
 *
 * **One three-valued variable rather than a boolean each.** Two booleans admit
 * `saving && importing`, a state that must never exist and that four separate
 * conditions would then have to agree about. O96 shipped two guards that
 * agreed by hand and O97 is what that cost — a save started during an import,
 * which no condition was asked about. This is #115's `phaseOf` lesson one size
 * down: when every fix moves one condition and the gates then disagree, the
 * defect is the absence of a single source of truth, not the gates.
 */
type Writer = 'save' | 'import' | undefined
```

- [ ] **Step 3: Replace the two flags with the one variable**

Delete `const [saving, setSaving] = useState(false)` (line 179) and
`const savingNow = useRef(false)` (line 185). In place of the ref:

```ts
  /** The synchronous gate — read before either writer starts. */
  const writer = useRef<Writer>(undefined)
  /** The same fact, for the render. */
  const [writing, setWriting] = useState<Writer>(undefined)
  /** A save specifically — what the Save button's own label reads. */
  const saving = writing === 'save'
```

Then rewrite the five sites, and **only** these five:

- `save`'s head (261-263):

```ts
    if (writer.current !== undefined) return
    writer.current = 'save'
    setWriting('save')
```

- `save`'s `.then` (270-271) and its `.catch` (323-324), both:

```ts
        writer.current = undefined
        setWriting(undefined)
```

- `doImport`'s guard (373): `if (savingNow.current) {` becomes
  `if (writer.current === 'save') {`
- the form's `onCommit` guard (515): `if (savingNow.current) {` becomes
  `if (writer.current === 'save') {`

Both of the last two are exactly equivalent today — `savingNow.current` was
true during a save and at no other time — which is what makes this step a
refactor rather than a change.

- [ ] **Step 4: Run the suite and confirm the same number passes**

```bash
cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx
```

Expected: the same count as Step 1, zero failures. A changed count here means
this step was not a refactor and something must be undone before continuing.

- [ ] **Step 5: Typecheck, because vitest does not**

```bash
cd web && npx tsc -b
```

Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add web/src/RuleEditor.tsx
git commit -m "O97 task 1: fold the save gate's four flags into one writer

Pure refactor. saving && importing becomes unrepresentable rather than an
invariant four conditions have to agree about."
```

---

### Task 2: Freeze the surfaces during an import

**Files:**
- Modify: `web/src/RuleList.tsx` (prop declaration, type, 7 reads, doc comment)
- Modify: `web/src/PreviewTabs.tsx` (prop declaration, type, 2 reads)
- Modify: `web/src/RuleEditor.tsx` (2 call sites, Save's `disabled`, Import's
  `disabled` and label, `doImport`'s gate)
- Modify: `web/src/RuleList.test.tsx:65,253,277,386,398`
- Modify: `web/src/PreviewTabs.test.tsx:106`
- Test: `web/src/RuleEditor.test.tsx`

**Interfaces:**
- Consumes: `writer`, `writing`, `saving` from Task 1.
- Produces: `RuleList` and `PreviewTabs` both take `frozen: boolean` in place
  of `saving: boolean`; nothing else about either component changes. Task 3
  reads `writer.current` in `save`; Task 4 reads it in the form's `onCommit`.

- [ ] **Step 1: Write the two fixtures and the failing tests**

Add to `web/src/RuleEditor.test.tsx`, inside `describe('RuleEditor')`,
immediately before `it('refuses an import that arrives while a save is in
flight…')`. **This fixture has been executed against the current code** — it
holds the import open, `land` delivers, and the first assertion below fails
with `expected false to be true`, which is O97 reproduced:

```tsx
  type ImportAnswer = {
    rules: Rule[]
    complaints: string[]
    reopened_proposals: number[]
  }

  /**
   * Holds an import open and returns its resolver — the mirror of `heldSave`.
   * The answer carries **different rules from the ones sent**, which is what
   * an import actually does: it mints fresh ids and replaces the list, so a
   * fixture echoing the sent rules back could not fail against an edit being
   * held across it rather than discarded.
   */
  const heldImport = async () => {
    let resolveImport: (value: ImportAnswer) => void = () => {}
    vi.mocked(rulesApi.importRules).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveImport = resolve
        }),
    )
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // Loud rather than silent, for `heldSave`'s own reason: an import that
    // never fired would make every test below assert the no-write-in-flight
    // path while claiming to assert the other one.
    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
    return { host, land: (a: ImportAnswer) => resolveImport(a) }
  }

  /**
   * What `heldImport`'s import answers with. `set_series` is what a rule row
   * draws — never `pattern` — so these two strings are how a test tells the
   * imported list from the loaded one on screen.
   */
  const importAnswer = (): ImportAnswer => ({
    rules: [
      rule({ id: 11, set_series: 'Imported A' }),
      rule({ id: 12, ordinal: 2, set_series: 'Imported B' }),
    ],
    complaints: [],
    reopened_proposals: [],
  })

  it('disables Save while an import is in flight, and re-enables it when the import lands — O97', async () => {
    const { host, land } = await heldImport()
    expect(host.querySelector<HTMLButtonElement>('button.save')?.disabled).toBe(
      true,
    )

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The control, without which the assertion above passes against a button
    // disabled for some other reason. Both halves are non-trivial: the notice
    // proves the resolver delivered, and the row proves the list it delivered
    // is the one on screen.
    const notice = [...host.querySelectorAll('.import p')]
      .map((n) => n.textContent ?? '')
      .find((t) => t.includes('imported rules are stored'))
    expect(notice).toBeDefined()
    expect(
      [...host.querySelectorAll('.rule-row')].some((r) =>
        r.textContent?.includes('Imported B'),
      ),
    ).toBe(true)
  })

  it('says Importing… and disables Import while an import is in flight, so a second pick cannot race the first — O97', async () => {
    const { host, land } = await heldImport()
    const importButton = host.querySelector<HTMLButtonElement>('.import button')
    expect(importButton?.textContent).toBe('Importing…')
    expect(importButton?.disabled).toBe(true)

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    const after = host.querySelector<HTMLButtonElement>('.import button')
    expect(after?.textContent).toBe('Import')
    expect(after?.disabled).toBe(false)
  })

  it('freezes the rule list and the override options while an import is in flight — O97', async () => {
    const { host, land } = await heldImport()
    // Per control, not per page: a page-level assertion cannot tell these
    // apart from the Save button it sits beside.
    expect(
      host.querySelector<HTMLInputElement>('.rule-row input[type="checkbox"]')
        ?.disabled,
    ).toBe(true)
    expect(
      host.querySelector<HTMLButtonElement>('.rule-row button.select')?.disabled,
    ).toBe(true)
    expect(host.querySelector<HTMLElement>('.rule-row')?.draggable).toBe(false)

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(
      host.querySelector<HTMLInputElement>('.rule-row input[type="checkbox"]')
        ?.disabled,
    ).toBe(false)
    expect(host.querySelector<HTMLElement>('.rule-row')?.draggable).toBe(true)
  })
```

- [ ] **Step 2: Run them and watch all three fail**

```bash
cd web && npx vitest run --project jsdom -t 'O97' src/RuleEditor.test.tsx
```

Expected: 3 failed. The first fails `expected false to be true` on Save's
`disabled`; the second fails on `'Import'` not being `'Importing…'`; the third
fails on the checkbox's `disabled`. **If any of the three passes here, stop** —
it is asserting something that was already true and proves nothing.

- [ ] **Step 3: Rename `RuleList`'s prop**

In `web/src/RuleList.tsx`, rename `saving` to `frozen` at the destructuring
(line 42), the type (line 59) and all seven reads — `draggable={!frozen}`
(121), `if (frozen) return` in `onDrop` (136), and the five `disabled={frozen}`
(157, 164, 191, 211, 244). Update the prop's own doc comment (line 55) so it
describes the wider fact:

```ts
  /**
   * A write is in flight — a save or an import. Every control here is frozen
   * for its duration, because both writers answer with a list that replaces
   * this one, so a gesture made inside the window is applied, previewed and
   * then overwritten with nothing on screen saying so.
   *
   * **A write, never `busy`:** a preview in flight is superseded by
   * `runPreview`'s sequence guard and is never lost, so a gesture during one
   * is correct and cheap. Freezing on `busy` would grey the list for ~281 ms
   * after every gesture, which is the rhythm the build design's §5.3 costed
   * out and kept.
   */
  frozen: boolean
```

- [ ] **Step 4: Rename `PreviewTabs`' prop**

In `web/src/PreviewTabs.tsx`, rename `saving` to `frozen` at the destructuring
(103), the type (116) and both reads — `disabled={busy || frozen}` at 392 and
400. Update the type's doc comment (112-115) to say "a write is in flight —
a save or an import" in place of "a save is in flight", and the prose comment
at 361 and 377-378 likewise.

- [ ] **Step 5: Update the six existing mount sites**

`web/src/RuleList.test.tsx`: `saving={false}` becomes `frozen={false}` at 65,
253, 277 and 386; `saving={true}` becomes `frozen={true}` at 398.
`web/src/PreviewTabs.test.tsx`: `saving={false}` becomes `frozen={false}` at
106.

- [ ] **Step 6: Wire the editor**

In `web/src/RuleEditor.tsx`, first declare the derived boolean beside
`saving` — this is its first reader, which is why it lands here and not in
Task 1 (`noUnusedLocals: true` fails the build on a local nothing reads):

```ts
  /** Any write at all — what every frozen surface reads. */
  const frozen = writing !== undefined
```

Then `saving={saving}` becomes `frozen={frozen}` at both call sites
(453 and 536). Save's `disabled` (592) becomes
`disabled={frozen || busy || preview === undefined}`. The Import button (603)
becomes:

```tsx
          <button
            type="button"
            disabled={frozen}
            onClick={() => fileInput.current?.click()}
          >
            {writing === 'import' ? 'Importing…' : 'Import'}
          </button>
```

The label is deliberately keyed on `writing === 'import'` and not on `frozen`:
during a *save* this button reads `Import` and is disabled, which is what
O96's own test asserts, and reading `Importing…` there would be a lie.

Then take the gate in `doImport`, replacing the `if (writer.current === 'save')`
block Task 1 left:

```ts
    if (writer.current !== undefined) {
      setImportNotice(
        writer.current === 'save'
          ? 'A save was still running, so the import was not applied — nothing was ' +
              'imported and nothing was changed. Try it again now the save has finished.'
          : 'An import was already running, so this one was not applied — nothing ' +
              'was imported twice. Try it again now the first has finished.',
      )
      return
    }
    writer.current = 'import'
    setWriting('import')
```

and clear the gate in both of `importRules`' handlers:

```ts
      .then((answer) => {
        writer.current = undefined
        setWriting(undefined)
        commit(answer.rules, undefined)
        // …the rest of the existing `.then` unchanged
      })
      .catch((err: unknown) => {
        writer.current = undefined
        setWriting(undefined)
        setFailure(message(err))
      })
```

- [ ] **Step 7: Run the three tests and the whole file**

```bash
cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx src/RuleList.test.tsx src/PreviewTabs.test.tsx
```

Expected: all pass, including O96's `disables Import while a save is in
flight`, which asserts the label is still `Import` during a save.

- [ ] **Step 8: Typecheck**

```bash
cd web && npx tsc -b
```

Expected: no output. This is what catches a mount site missed in Step 5.

- [ ] **Step 9: Commit**

```bash
git add web/src/RuleEditor.tsx web/src/RuleList.tsx web/src/PreviewTabs.tsx \
        web/src/RuleEditor.test.tsx web/src/RuleList.test.tsx web/src/PreviewTabs.test.tsx
git commit -m "O97 task 2: freeze the same surfaces during an import that a save freezes

Renames both components' saving prop to frozen rather than passing a lie
under the old name, and takes the gate in doImport — which closes a second
window nobody had numbered, where Import was live during an import."
```

---

### Task 3: The refusal notice, and the guard the button cannot be

**Files:**
- Modify: `web/src/RuleEditor.tsx` (`refused` state, `save`'s gate, the render)
- Test: `web/src/RuleEditor.test.tsx`

**Interfaces:**
- Consumes: `writer`, `writing` (Task 1), `frozen` (Task 2); `heldImport`,
  `importAnswer`, `ImportAnswer` (Task 2).
- Produces: `refused: string | undefined` state and `setRefused`, which Task 4
  also writes to. Rendered as `p.refused` with `role="alert"`.

- [ ] **Step 1: Write the two failing tests**

Add after Task 2's three, inside the same `describe`. **The window the first
one drives has been executed against the current code**: after two microtask
ticks `doImport` has run and the Save button still fires, so
`saveRules` is called once today and must be called zero times once the guard
lands.

```tsx
  it('refuses a save that slips past the disabled button during an import, and writes nothing — O97', async () => {
    // The *second* guard, for the reason the drag has two. The button's
    // `disabled` is state and lands on a render; `writer` is read
    // synchronously. Between `setWriting('import')` and the DOM update there
    // is a real window in which a click reaches an enabled button — the
    // mirror of the window `savingNow` was a ref for. Driving it here is what
    // makes this test asymmetric to the `disabled` assertion above, which is
    // what proves the two guards are not one test twice.
    let resolveImport: (value: ImportAnswer) => void = () => {}
    vi.mocked(rulesApi.importRules).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveImport = resolve
        }),
    )
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const saveButton = host.querySelector<HTMLButtonElement>('button.save')
    if (!saveButton) throw new Error('no save button')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })

    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      // Two ticks: the first for `file.text()`, the second for `doImport`.
      // After them the import is in flight and React has not yet committed
      // the render that disables the button.
      await Promise.resolve()
      await Promise.resolve()
      saveButton.click()
      await Promise.resolve()
    })

    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
    expect(rulesApi.saveRules).not.toHaveBeenCalled()
    expect(host.querySelector('p.refused')?.textContent).toContain(
      'was not sent',
    )

    await act(async () => {
      resolveImport(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
  })

  it('keeps the refusal on screen after the import that caused it lands — O97', async () => {
    // §5 of the design, executed rather than asserted. `commit` clears
    // `heldNotice` and `doImport`'s success path overwrites `importNotice`,
    // so a refusal living in either slot disappears at exactly the moment the
    // person needs it — leaving them told the import worked and never told
    // their save did nothing. This test fails if the notice is moved.
    let resolveImport: (value: ImportAnswer) => void = () => {}
    vi.mocked(rulesApi.importRules).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveImport = resolve
        }),
    )
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const saveButton = host.querySelector<HTMLButtonElement>('button.save')
    if (!saveButton) throw new Error('no save button')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      saveButton.click()
      await Promise.resolve()
    })
    expect(host.querySelector('p.refused')).not.toBeNull()

    await act(async () => {
      resolveImport(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The import landed, `commit` ran, and the import's own notice is up —
    // and the refusal is still there beside it.
    expect(host.querySelector('p.refused')?.textContent).toContain(
      'was not sent',
    )
    expect(
      [...host.querySelectorAll('.import p')]
        .map((n) => n.textContent ?? '')
        .find((t) => t.includes('imported rules are stored')),
    ).toBeDefined()
  })
```

- [ ] **Step 2: Run them and watch both fail**

```bash
cd web && npx vitest run --project jsdom -t 'O97' src/RuleEditor.test.tsx
```

Expected: the two new ones fail on `expected "saveRules" to not be called` and
on `p.refused` being null. Task 2's three still pass.

- [ ] **Step 3: Add the slot**

In `web/src/RuleEditor.tsx`, beside the other notice states:

```ts
  /**
   * A gesture refused or discarded because a write was in flight.
   *
   * **Its own slot, because neither existing one can hold it.** `heldNotice`
   * is cleared by `commit`, by design and with its own comment saying why —
   * and the import's success path calls `commit`, so a refusal written there
   * is wiped by the very event it has to outlive. `importNotice` is cleared up
   * front by `doImport` and overwritten by its answer, so a refused save
   * written there survives until the import lands and then disappears, leaving
   * the person told the import worked and never told their save did nothing.
   *
   * This is the fourth notice on this screen needing a slot `runPreview` does
   * not touch, after `importNotice`, `saveMismatch` and `heldNotice`, and the
   * pattern is worth naming: a message about a gesture that did *not* happen
   * cannot live in a slot owned by the gesture that did.
   *
   * Cleared when a write actually starts, which is the moment its claim stops
   * being true — and deliberately not by `commit`.
   */
  const [refused, setRefused] = useState<string | undefined>()
```

- [ ] **Step 4: Take the gate in `save`**

Replace `save`'s head (Task 1's version) with:

```ts
  const save = () => {
    if (writer.current !== undefined) {
      // Only the import case says anything: a second Save during a save is
      // the same gesture twice and the person has nothing to act on, which is
      // the behaviour O96 shipped and this keeps.
      if (writer.current === 'import')
        setRefused(
          'An import was still running, so the save was not sent — nothing was saved ' +
            'and nothing was changed. The imported rules are on screen now; press Save ' +
            'to apply them.',
        )
      return
    }
    writer.current = 'save'
    setWriting('save')
    setRefused(undefined)
```

- [ ] **Step 5: Clear it when an import starts too**

In `doImport`, immediately after `writer.current = 'import'` and
`setWriting('import')`, add `setRefused(undefined)`.

- [ ] **Step 6: Render it**

Beside the `heldNotice` block (around line 441):

```tsx
      {refused !== undefined && (
        <p role="alert" className="refused">
          {refused}
        </p>
      )}
```

- [ ] **Step 7: Run and confirm both pass**

```bash
cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx
```

Expected: all pass.

- [ ] **Step 8: Hand-apply each defect and watch the test fail alone**

Two mutations, run one at a time and reverted between:

1. Delete the `if (writer.current === 'import') setRefused(...)` block, keeping
   the bare `return`. Expected: *refuses a save that slips past* fails on
   `p.refused`, *keeps the refusal on screen* fails, and **nothing else does**.
2. Change `setRefused(...)` in `save` to `setHeldNotice(...)` and render it
   from `heldNotice`. Expected: *keeps the refusal on screen* fails — `commit`
   wipes it — and *refuses a save that slips past* still passes. That
   asymmetry is §5's argument executed, and if both fail together the second
   test is not pinning what it claims.

- [ ] **Step 9: Commit**

```bash
git add web/src/RuleEditor.tsx web/src/RuleEditor.test.tsx
git commit -m "O97 task 3: a refused save says so, in a slot the import cannot wipe"
```

---

### Task 4: The form commit's disposition splits by writer

**Files:**
- Modify: `web/src/RuleEditor.tsx:515` (the form's `onCommit`)
- Test: `web/src/RuleEditor.test.tsx`

**Interfaces:**
- Consumes: `writer` (Task 1), `setRefused` (Task 3), `heldImport` /
  `importAnswer` (Task 2), and the existing `heldSave`, `heldSaveAnswer` and
  `editPatternAndBlur` fixtures.
- Produces: nothing later tasks read.

- [ ] **Step 1: Write the failing test and its regression partner**

```tsx
  it('discards a form commit made during an import rather than holding it, and says which rule — O97', async () => {
    // Holding works for a save because `answer.rules` is positionally aligned
    // to the list that was sent. An import's answer is aligned to nothing: it
    // replaces every rule and mints fresh ids, so index 1 of the answer is not
    // the rule the person was editing and applying the edit there would
    // overwrite an unrelated rule with it. Discarding is forced, not chosen.
    const { host, land } = await heldImport()
    // Open a rule first — `heldImport` leaves nothing selected.
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await editPatternAndBlur(host, 'Edited/.*')

    expect(host.querySelector('p.refused')?.textContent).toContain('rule 2')

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The imported list is what is on screen, with no trace of the edit.
    expect(
      [...host.querySelectorAll('.rule-row')].some((r) =>
        r.textContent?.includes('Imported B'),
      ),
    ).toBe(true)
  })

  it('still holds a form commit made during a save, which this branch must not cost — O96, kept', async () => {
    // The regression guard: task 4 changes the code that decides between held
    // and discarded, and O96's whole claim lives on the other branch of it.
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('p.refused')).toBeNull()
    const pattern = host.querySelector<HTMLInputElement>('input[name="pattern"]')
    expect(pattern?.value).toBe('Edited/.*')
  })
```

- [ ] **Step 2: Run and watch the first fail, the second pass**

```bash
cd web && npx vitest run --project jsdom -t 'O9' src/RuleEditor.test.tsx
```

Expected: *discards a form commit* fails on `p.refused` being null (the commit
falls through to `commit`, which applies it). *still holds a form commit*
**passes already** — it is a regression guard, and a guard that fails before
the change it guards against is testing the wrong thing.

- [ ] **Step 3: Split the disposition**

In the `RuleForm`'s `onCommit` (line 515), replacing Task 1's version:

```tsx
                // O96: `RuleForm` is deliberately not frozen — disabling a
                // focused input makes the browser fire `blur`, and this form
                // commits on blur, so the mechanical guard would itself commit
                // the half-typed edit into the very window the item is about.
                // The field stays live and the *commit* is what the gate acts
                // on — with two answers, because the two writers answer with
                // differently-shaped lists.
                if (writer.current === 'save') {
                  held.current = { index: selected, rule: edited }
                  return
                }
                if (writer.current === 'import') {
                  // Discarded, not held. An import mints fresh ids and
                  // replaces every rule, so there is no correspondence to
                  // carry the edit across — applying it at this index would
                  // overwrite an unrelated rule with it.
                  setRefused(
                    `An import was running, so your edit to rule ${selected + 1} was not ` +
                      'applied — an import replaces every rule, so there is no row left to ' +
                      'apply it to. The imported rules are on screen now.',
                  )
                  return
                }
```

- [ ] **Step 4: Run and confirm both pass**

```bash
cd web && npx vitest run --project jsdom src/RuleEditor.test.tsx
```

Expected: all pass.

- [ ] **Step 5: Hand-apply the defect**

Change `if (writer.current === 'import')` to `if (false)` so the commit falls
through. Expected: *discards a form commit* fails and *still holds a form
commit* passes. Revert.

Then swap the two branches — `'import'` held, `'save'` discarded. Expected:
both fail. If only one does, one of them is not reading the branch it names.

- [ ] **Step 6: Commit**

```bash
git add web/src/RuleEditor.tsx web/src/RuleEditor.test.tsx
git commit -m "O97 task 4: a form commit during an import is discarded, not held"
```

---

### Task 5: The gate, the measurement, the drive, and the five-part close

**Files:**
- Modify: `web/src/RuleEditor.tsx` (the file's own doc comment)
- Modify: `docs/notes.md`
- Modify: `docs/status.md`

**Interfaces:**
- Consumes: everything above.
- Produces: the branch's shipped state.

- [ ] **Step 1: Rewrite the file's doc comment**

`RuleEditor.tsx`'s header carries O96's paragraphs at lines 52-101 describing
two guards that happen to agree — Save's `busy`, the held commit, and "Import
is the third surface and is *refused* rather than frozen or held". Rewrite
them as **one gate with two writers**: name `writer`, say why it is
three-valued, say that Import is refused in both directions and that a form
commit is held for a save and discarded for an import, and say why. A comment
asserting a guard that does not exist was O96's own second finding; a comment
describing a guard that has been replaced is the same failure.

- [ ] **Step 2: Run the full gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: its own `all green` line. Quote it in the commit or PR body —
piping through `tail` or appending `; echo` masks a non-zero exit.

- [ ] **Step 3: Measure the import window against a copy of the real library**

The design's §0.2 owes this figure. Serve a **copy**, never the real tree —
`/home/davey/temp/manga-test`, with the root repointed to an empty scratch
directory first, because an import followed by a save re-derives and writes
`<root>/.taiyaku/`. Time `POST /api/library/rules/import` from the browser's
own network panel, ten runs, and record the median in `docs/notes.md` beside
O96's 280.7 ms save. Prove isolation afterwards rather than assuming it: the
real `library.db`'s mtime unchanged, nothing under the real tree newer than
the start stamp.

- [ ] **Step 4: Drive the window with the import held open**

Hold the POST open at the transport, then, against the real copy: confirm the
rule list is frozen and the row `draggable="false"`, that Save is disabled and
reads its ordinary label, that the Import button reads `Importing…`, that a
blur on an open rule produces the discard notice naming that rule, and that
when the import lands the imported rules are on screen with the refusal notice
still standing beside the import's own. Record it in `docs/notes.md`. A
fixture and a real library disagreeing is what the drive exists to find.

- [ ] **Step 5: Close O97 in `docs/status.md` — five parts, and a sweep**

1. The table row's checkbox: `| O97 | [ ]` becomes `| O97 | [x]`.
2. The row's prose, rewritten to say what was done, that the two unnumbered
   losses in §0.1 closed with it, and what remains — the design's §7 third
   bullet: nothing enforces that a future writer takes the gate.
3. The test count, which appears **twice** — in "Six crates, 2070 tests, plus
   2,067 in `web/`'s TypeScript" and again in the `node`/`jsdom` split two
   lines later. Read the real numbers off the run, never scale them.
4. "**Forty-nine** are now closed" becomes **Fifty**.
5. "What remains: the **fifty** open items" becomes **forty-nine** — the count
   word **wraps across a line break**, so `grep 'forty-nine open items'` finds
   nothing and reads as already-updated. Grep the sentence opener and take the
   next line. The clause after it says how many came from a whole-branch
   review; O97 did, so that number moves too.

The head count word ("Ninety-nine") does **not** move: no row opens here.

Then the sweep, on the way out:

```bash
cd /home/davey/projects/taiyaku
echo "rows: $(grep -c '^| O[0-9]' docs/status.md)  closed: $(grep -c '^| O[0-9]* | \[x\]' docs/status.md)"
grep -n -A1 '^What remains:' docs/status.md
grep -rn 'O97' docs/ web/src/
```

The last one is the part the five do not cover: prose *outside* the table
describes open items in wording no grep can predict. Read every hit.

- [ ] **Step 6: Commit and open the PR**

```bash
git add -A && git commit -m "O97: close it, with the measurement and the five-part edit"
git push -u origin o97-import-window
```

The forge is **Gitea, not GitHub** — `gh` is authenticated against the wrong
host. Open the PR with the gitea MCP. The remote is squash-only, so the PR body
*is* the squash commit message: write it for a reader of `main`'s history, do
not hard-wrap it, and put no attribution line in it.

- [ ] **Step 7: Review the branch whole, and review the fixes too**

Per-task review passed on every task of #91 and #105 while whole-branch review
found seven Criticals and two respectively. On #108 four passes did not
converge and seven findings came from earlier fix rounds; on #112 passes 2-4
found *only* defects inside earlier fixes. So: review the whole branch, then
review each fix round, and after fixing a finding grep for its *shape* rather
than its function. Do not dispatch a subagent to run the gate or the sweep.

## Self-Review

**Spec coverage.** §1 refuse-not-queue → Tasks 2 and 3. §2 the one variable →
Task 1. §3 renamed props and both `disabled` widenings → Task 2. §4 the split
disposition → Task 4. §5 the notice slot → Task 3. §6's seven tests → three in
Task 2, two in Task 3, two in Task 4. §7's non-goals are carried into Task 5's
prose. §8's three documents → Task 5. §0.2's owed measurement → Task 5 Step 3.

**Type consistency.** `Writer`, `writer`, `writing`, `setWriting`, `saving`,
`frozen`, `refused`, `setRefused`, `ImportAnswer`, `heldImport`, `importAnswer`
are spelled identically at every use across the five tasks. `frozen` is the
prop name in both components and the local name in `RuleEditor`.

**Executed rather than asserted.** `heldImport`, `importAnswer`, the two-tick
microtask window and the `set_series`-not-`pattern` row assertion were all run
against the current code before this plan was written; the first draft of the
row assertion matched on `pattern` and was wrong. What is **not** executed and
is marked as owed: the post-fix assertions, which cannot run until the fix
exists, and Task 5's measurement.

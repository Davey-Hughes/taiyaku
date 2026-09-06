# O96 — the rule editor's save and work in flight: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A save no longer silently discards editor gestures made while it is in
flight: the rule list freezes for the duration, and a form commit that arrives
during it is applied on top of the server's answer rather than overwritten by it.

**Architecture:** `RuleList` gains a `saving` prop and disables its five
gestures; `RuleForm` is untouched, so nothing is swallowed and no focus is
stolen; `RuleEditor` holds at most one form commit in a ref and merges it into
`answer.rules` on landing, taking `id` and `ordinal` from the answer so the
server's minted ids survive. A notice in its own state slot says it happened.

**Tech Stack:** React 19, TypeScript, Vitest (`--project node`, jsdom
environment), Biome.

## Global Constraints

- The design is
  [`docs/superpowers/specs/2026-09-06-o96-save-in-flight-design.md`](../specs/2026-09-06-o96-save-in-flight-design.md).
  Where this plan and that spec disagree, the spec is wrong and Task 1 fixes it.
- **Never `busy`, always `saving`.** A preview in flight is superseded by
  `runPreview`'s `latest` sequence guard and is never lost; only a save
  overwrites the list. Gating anything here on `busy` greys the screen for
  ~281 ms after every gesture.
- **Assert per element.** A whole-page `toContain` cannot see a pairing. Every
  assertion below names the element it is about.
- **Every test is hand-applied against its own defect before it is believed.**
  Break the production line the test names, watch that test fail, put it back.
  Eleven tests on C4 PR B passed against their own defect and none was visible
  to reading.
- Run tests with `pnpm --dir web test:jsdom <file-filter>` — the **jsdom** project, not `test`:
  `vite.config.ts` gives `node` `src/**/*.test.ts` and `jsdom`
  `src/**/*.test.tsx`, so `pnpm --dir web test RuleList` answers *No test files
  found* rather than running anything. Do **not** `cd` — `cd` is
  zoxide in this shell and a failed `cd` aborts a chain silently.
- No attribution lines in commit messages.

---

### Task 1: Amend the spec — what a *failed* save does with a held edit

The spec settles the success path and is silent on the failure path. Making the
steps exact surfaced that the two paths cannot share one answer, because
`runPreview`'s success path clears `failure`. Fix the spec before writing code
against it.

**Files:**
- Modify: `docs/superpowers/specs/2026-09-06-o96-save-in-flight-design.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the decision Task 4's `.catch` implements.

- [ ] **Step 1: Read §5 of the spec**

Run: `sed -n '/## 5. The notice/,/## 6. Tests/p' docs/superpowers/specs/2026-09-06-o96-save-in-flight-design.md`

- [ ] **Step 2: Append this subsection to §5, immediately before `## 6. Tests`**

```markdown
### 5.1 A failed save, and why it cannot share the success path's answer

**Found writing the implementation plan, 2026-09-06; this section is an
amendment and the sections above were written without it.**

A save that fails wrote nothing, so the list on screen is still the one that
was sent and a held edit applies to it unchanged — there is no minted id to
adopt. But applying it means committing it, and `commit` runs a preview whose
success path calls `setFailure(undefined)`. The save's own failure text —
including the **409** `save_rules` answers when a scan holds `library_job`,
which is the one a person most needs to read — would be wiped ~281 ms later by
the preview run to describe the list the edit just changed.

**So on the failure path the message goes to `heldNotice`, not to `failure`,
and carries both facts:** the save failed, with its reason, and the edit
arrived during it and is applied and unsaved.

**This branch is reached only when a held edit exists.** A failed save with
nothing held keeps `setFailure(message(err))` exactly as today, because nothing
follows it to wipe it. The conditional is therefore not "two slots for one
error" — it is one slot per *outcome*, chosen by whether a preview follows.

**Rejected: apply the edit without previewing it.** `failure` would survive,
and the Save button would then state a `works_rehomed` computed for a list that
is no longer `rules` — the exact defect I2 closed for a failed preview,
reintroduced through a failed save.

**Rejected: drop the held edit when the save fails.** It is a real edit a
person made, and discarding it silently is the whole of what O96 is.
```

- [ ] **Step 3: Verify no line exceeds 80 characters except the two link lines**

Run: `awk 'length > 80 {print FNR": "length}' docs/superpowers/specs/2026-09-06-o96-save-in-flight-design.md`
Expected: exactly two lines, `19` and `21` (the markdown links).

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-09-06-o96-save-in-flight-design.md
git commit -m "O96 spec §5.1: what a failed save does with a held edit

A failed save wrote nothing, so a held edit applies to the list unchanged
— but applying it runs a preview, whose success path clears \`failure\`,
which would wipe the save's own reason (the 409 a scan holds most of all).
On that path the message goes to the notice slot and carries both facts."
```

---

### Task 2: `RuleList` freezes while a save is in flight

**Files:**
- Modify: `web/src/RuleList.tsx`
- Test: `web/src/RuleList.test.tsx`

**Interfaces:**
- Consumes: nothing.
- Produces: `RuleList` takes a required `saving: boolean` prop. Task 4 passes it.

- [ ] **Step 1: Write the two failing tests**

Add to `web/src/RuleList.test.tsx`, inside the `describe('RuleList', ...)` block,
after the existing `it('adds a rule', ...)`:

```tsx
  /**
   * The five gestures O96 freezes, read **per element**. A whole-page check
   * cannot tell "the row is frozen" from "the page rendered nothing".
   */
  const gestures = (host: HTMLElement) => ({
    draggable: host
      .querySelector<HTMLElement>('.rule-row')
      ?.getAttribute('draggable'),
    toggle: host.querySelector<HTMLInputElement>(
      '.rule-row input[type="checkbox"]',
    )?.disabled,
    select: host.querySelector<HTMLButtonElement>('.rule-row button.select')
      ?.disabled,
    remove: host.querySelector<HTMLButtonElement>('.rule-row button.delete')
      ?.disabled,
    add: host.querySelector<HTMLButtonElement>('button.add-rule')?.disabled,
  })

  it('freezes every gesture while a save is in flight, and only while one is — O96', () => {
    // Both halves, because the positive alone passes against a component that
    // hardcodes `disabled` — which is the mechanical mis-fix this item warns
    // about, and it would freeze the list permanently.
    expect(gestures(render({ saving: true }).host)).toEqual({
      draggable: 'false',
      toggle: true,
      select: true,
      remove: true,
      add: true,
    })
    expect(gestures(render({ saving: false }).host)).toEqual({
      draggable: 'true',
      toggle: false,
      select: false,
      remove: false,
      add: false,
    })
  })

  it('refuses a drop that lands while a save is in flight — O96', () => {
    // `draggable={!saving}` cannot cancel a drag that was already in progress
    // when the save started; flipping the attribute under it does nothing to
    // the drop that ends it. jsdom dispatches both events regardless of the
    // attribute, which is exactly what lets this assert the **handler's**
    // guard rather than the attribute's — remove the `if (saving) return`
    // from `onDrop` and this fails while the test above still passes.
    const { host, onReorder } = render({ saving: true })
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    const dt = dataTransfer()
    act(() => {
      rows[1].dispatchEvent(
        Object.assign(new Event('dragstart', { bubbles: true }), {
          dataTransfer: dt,
        }),
      )
      rows[0].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: dt,
          preventDefault: () => {},
        }),
      )
    })
    expect(onReorder).not.toHaveBeenCalled()
  })
```

Then give the file's `render` helper a default, so the other twenty tests keep
compiling. Change:

```tsx
      <RuleList
        rules={[rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })]}
        selected={0}
        proposals={[]}
        overrideCount={0}
        {...handlers}
        {...props}
      />,
```

to:

```tsx
      <RuleList
        rules={[rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })]}
        selected={0}
        proposals={[]}
        overrideCount={0}
        saving={false}
        {...handlers}
        {...props}
      />,
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir web test:jsdom RuleList`
Expected: FAIL. The first with `draggable` `'true'` where `'false'` was expected
and four `undefined`/`false` mismatches; the second with `onReorder` called
`(1, 0)`. A TypeScript error on the unknown `saving` prop is also expected and
is part of the failure.

- [ ] **Step 3: Add the prop and the guards to `RuleList.tsx`**

Add `saving` to the destructured parameters, between `overrideCount` and
`onSelect`:

```tsx
export function RuleList({
  rules,
  selected,
  proposals,
  overrideCount,
  saving,
  onSelect,
  onToggle,
  onReorder,
  onAdd,
  onDelete,
}: {
  rules: Rule[]
  selected: number | undefined
  proposals: Proposal[]
  overrideCount: number
  /**
   * A save is in flight, and every gesture here is frozen for the duration —
   * O96, and this file's own doc comment on why. **`saving`, never `busy`:**
   * a preview in flight is superseded by `RuleEditor`'s `latest` sequence
   * guard and is never lost, so a gesture during one is correct and cheap.
   */
  saving: boolean
  onSelect: (index: number) => void
  onToggle: (index: number) => void
  onReorder: (from: number, to: number) => void
  onAdd: () => void
  onDelete: (index: number) => void
}) {
```

On the `<li>`, replace `draggable` and add the drop guard:

```tsx
          <li
            key={index}
            className="rule-row"
            aria-current={selected === index ? 'true' : undefined}
            draggable={!saving}
            onDragStart={(e) => {
              e.dataTransfer.setData('text/plain', String(index))
              e.dataTransfer.effectAllowed = 'move'
            }}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault()
              // O96, and **both guards are needed**: `draggable={!saving}`
              // stops a drag *starting* during a save, and this stops the
              // drop of one that was already in progress when the save
              // began — flipping the attribute under a live drag cancels
              // nothing. A reorder committed here is overwritten by
              // `answer.rules` when the save lands, with nothing on screen
              // saying so, which is the whole of O96.
              if (saving) return
              const raw = e.dataTransfer.getData('text/plain')
              if (raw === '') return
              const from = Number(raw)
              if (Number.isNaN(from) || from === index) return
              onReorder(from, index)
            }}
          >
```

Add `disabled={saving}` to four controls. The row's checkbox:

```tsx
            <input
              type="checkbox"
              checked={rule.enabled}
              disabled={saving}
              aria-label={`rule ${index + 1} enabled`}
              onChange={() => onToggle(index)}
            />
```

The row's select button:

```tsx
            <button
              type="button"
              className="select"
              disabled={saving}
              onClick={() => onSelect(index)}
            >
```

The row's delete button:

```tsx
            <button
              type="button"
              className="delete"
              disabled={saving}
              aria-label={`delete rule ${index + 1}`}
              onClick={() => askOrDelete(index)}
            >
              Delete
            </button>
```

The confirm dialog's destructive button — it can be open when a save starts,
and it is the one gesture here that reopens a proposal:

```tsx
          <button
            type="button"
            className="confirm-delete"
            disabled={saving}
            onClick={() => {
              onDelete(confirming.index)
              setConfirming(undefined)
            }}
          >
            Delete and reopen
          </button>
```

The add button:

```tsx
      <button type="button" className="add-rule" disabled={saving} onClick={onAdd}>
        + Add
      </button>
```

- [ ] **Step 4: Add the paragraph to `RuleList.tsx`'s doc comment**

Insert before the closing ` */` of the file's top doc comment, after the
"Deleting a rule an accepted proposal wrote" paragraph:

```
 * **Every gesture here is frozen while a save is in flight — O96.** Not
 * politeness: `RuleEditor::save` replaces the whole list with `answer.rules`
 * when it lands, so a reorder, a toggle, an add or a delete committed during
 * the ~281 ms it takes is applied, previewed, and then silently thrown away.
 * The freeze is also what keeps the positional identity above *true across a
 * save* — `answer.rules` is aligned to the list that was sent, and a list that
 * moved under the request has no correspondence to it at all. `RuleForm` is
 * deliberately **not** frozen; that half is in `RuleEditor.tsx`'s own comment.
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm --dir web test:jsdom RuleList`
Expected: PASS, all tests in the file.

- [ ] **Step 6: Hand-apply each defect and watch the right test fail**

Run three times, restoring the file between each:

1. Change `draggable={!saving}` to `draggable`. Run `pnpm --dir web test:jsdom RuleList`.
   Expected: the *freezes every gesture* test fails on `draggable`; the *refuses
   a drop* test still passes. That asymmetry is the point — it proves the two
   tests cover different guards.
2. Delete `if (saving) return` from `onDrop`. Run again.
   Expected: only *refuses a drop* fails.
3. Change one `disabled={saving}` to `disabled`. Run again.
   Expected: *freezes every gesture* fails on its second half — the
   `saving: false` control — not its first.

If any of these three passes, the test is not testing what it claims and must be
fixed before continuing.

- [ ] **Step 7: Commit**

```bash
git add web/src/RuleList.tsx web/src/RuleList.test.tsx
git commit -m "O96: the rule list freezes while a save is in flight

Both guards, not one: draggable={!saving} stops a drag starting during a
save, and the onDrop guard stops the drop of one already in progress when
it began — flipping the attribute under a live drag cancels nothing. On
saving and never on busy: a preview in flight is superseded by the seq
guard and never lost, so a gesture during one is correct and cheap."
```

---

### Task 3: `RuleEditor` holds one form commit and merges it on landing

**Files:**
- Modify: `web/src/RuleEditor.tsx`
- Test: `web/src/RuleEditor.test.tsx`

**Interfaces:**
- Consumes: `RuleList`'s `saving: boolean` prop from Task 2.
- Produces: `held: React.RefObject<{ index: number; rule: Rule } | undefined>`
  and a `heldNotice: string | undefined` state slot, both private to
  `RuleEditor`. Nothing outside the file consumes them.

- [ ] **Step 1: Write the four failing tests**

Add to `web/src/RuleEditor.test.tsx`, inside `describe('RuleEditor', ...)`,
after the existing `it('refuses a second Save while the first is still in
flight, ...')`:

```tsx
  /**
   * Holds a save open and returns its resolver. The list this fixture loads
   * has an **unsaved second rule** (`id: 0`) that comes back with a minted
   * `id: 7` — a fixture returning the ids it sent cannot fail against the
   * merge taking the edit's own id, which is the defect that reopens
   * proposals for nothing.
   */
  const heldSave = async () => {
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule(), rule({ id: 0, ordinal: 2, pattern: 'Colour/.*' })],
      complaints: [],
    })
    let resolveSave: (value: Saved) => void = () => {}
    let rejectSave: (why: unknown) => void = () => {}
    vi.mocked(rulesApi.saveRules).mockImplementation(
      () =>
        new Promise((resolve, reject) => {
          resolveSave = resolve
          rejectSave = reject
        }),
    )
    const host = await render()
    // Open rule 2 — the unsaved one — so the form's commit lands on it.
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
      await Promise.resolve()
    })
    const button = host.querySelector<HTMLButtonElement>('button.save')
    if (!button) throw new Error('no save button')
    await act(async () => {
      button.click()
      await Promise.resolve()
    })
    // Loud rather than silent: the button is disabled on `busy`, and a
    // click that never fired would otherwise make every test below assert
    // the no-save-in-flight path while claiming to assert the other one.
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    // `land` and `fail` read the `let`s above at call time, which is after
    // the executor assigned them.
    return {
      host,
      land: (answer: Saved) => resolveSave(answer),
      fail: (why: unknown) => rejectSave(why),
    }
  }

  /** Blur a changed `pattern` field, which is what commits it. */
  const editPatternAndBlur = async (host: HTMLElement, value: string) => {
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    act(() => {
      setter?.call(pattern, value)
      pattern.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await act(async () => {
      pattern.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
    })
  }

  it('keeps a form commit made while a save was in flight, on the server’s own row — O96', async () => {
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      land(
        savedBody({
          rules: [rule(), rule({ id: 7, ordinal: 2, pattern: 'Colour/.*' })],
        }),
      )
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    // The edit survived the save that would have overwritten it...
    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('Edited/.*')
    // ...and the *server's* minted id came with it. Asserted through the next
    // preview's own argument, which is the list `rules` now holds — reading
    // it off the DOM is impossible, and that is precisely why taking the
    // edit's `id: 0` forward would ship unnoticed.
    const sent = vi.mocked(rulesApi.previewRules).mock.calls.at(-1)?.[0]
    expect(sent?.[1]).toMatchObject({ id: 7, pattern: 'Edited/.*', ordinal: 2 })
  })

  it('says the held edit is applied and unsaved, and the notice outlives the follow-up preview — O96', async () => {
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      land(
        savedBody({
          rules: [rule(), rule({ id: 7, ordinal: 2, pattern: 'Colour/.*' })],
        }),
      )
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    // The follow-up preview has landed by now, and its success path calls
    // `setFailure(undefined)` — a notice stored in `failure` would be gone.
    const notice = host.querySelector('.held-edit')
    expect(notice?.textContent).toContain('rule 2')
    expect(notice?.textContent).toContain('unsaved')
    // The save's own report is still standing next to it, which is the whole
    // reason the notice exists rather than `commit`'s `setSaved(undefined)`.
    expect(host.querySelector('.saved')).not.toBeNull()
  })

  it('replaces the list with the server’s answer when nothing was edited during the save — O96', async () => {
    // The unchanged path. Without this, "keep the edit" could be implemented
    // as "never replace the list" and every other test here would still pass,
    // while unsaved rules kept `id: 0` forever.
    const { host, land } = await heldSave()
    await act(async () => {
      land(
        savedBody({
          rules: [rule(), rule({ id: 7, ordinal: 2, pattern: 'Colour/.*' })],
        }),
      )
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.held-edit')).toBeNull()
    const sent = vi.mocked(rulesApi.previewRules).mock.calls.at(-1)?.[0]
    expect(sent?.[1]).toMatchObject({ id: 7, pattern: 'Colour/.*' })
  })

  it('keeps a held edit when the save fails, and states the failure where the preview cannot wipe it — O96', async () => {
    // Spec §5.1. `commit` fires a preview whose success path clears
    // `failure`, so the save's own reason goes to the notice slot on this
    // path — otherwise the 409 a scan causes flashes and vanishes.
    const { host, fail } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      fail(new Error('a scan is currently running'))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('Edited/.*')
    const notice = host.querySelector('.held-edit')
    expect(notice?.textContent).toContain('a scan is currently running')
    expect(notice?.textContent).toContain('rule 2')
  })
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir web test:jsdom RuleEditor`
Expected: FAIL — the first with `pattern` reading `'Colour/.*'` (the answer
overwrote the edit), the second and fourth with `notice` `null`, the third
passing already (it asserts today's behaviour and is the regression guard).

- [ ] **Step 3: Add the ref, the state slot, and the `saving` prop**

In `RuleEditor.tsx`, after the `saveMismatch` state declaration, add:

```tsx
  /**
   * Set when a `RuleForm` commit arrives while a save is in flight. Its own
   * slot for the reason `saveMismatch` and `importNotice` each have one: the
   * follow-up preview clears `failure` when it succeeds, and a notice stored
   * there is wiped by the very call that follows it. Cleared only by the next
   * save. Spec §5, and §5.1 for why the *failed*-save reason lands here too.
   */
  const [heldNotice, setHeldNotice] = useState<string | undefined>()
```

After the `savingNow` ref, add:

```tsx
  /**
   * A `RuleForm` commit that arrived while a save was in flight — O96.
   *
   * The list is frozen for the duration (`RuleList`'s `saving` prop), so the
   * form is the one surface that can still commit, `selected` cannot move
   * under it, and at most one rule can be edited. Held rather than applied,
   * because `save`'s own `.then` replaces the whole list with `answer.rules`;
   * applied on landing, onto the server's row, so the minted id survives.
   */
  const held = useRef<{ index: number; rule: Rule } | undefined>(undefined)
```

Pass the prop to `RuleList`, after `overrideCount`:

```tsx
          overrideCount={preview?.overrides.length ?? 0}
          saving={saving}
```

- [ ] **Step 4: Hold the form's commit**

Replace `RuleForm`'s `onCommit` handler:

```tsx
              onCommit={(edited) => {
                // O96: `RuleForm` is deliberately not frozen — disabling a
                // focused input makes the browser fire `blur`, and this form
                // commits on blur, so the mechanical guard would itself
                // commit the half-typed edit into the very window the item is
                // about. So the field stays live and the *commit* waits.
                if (savingNow.current) {
                  held.current = { index: selected, rule: edited }
                  return
                }
                commit(
                  rules.map((rule, i) => (i === selected ? edited : rule)),
                  selected,
                )
              }}
```

- [ ] **Step 5: Merge on landing, and on failure**

Replace the whole of `save`'s `.then` and `.catch`:

```tsx
      .then((answer) => {
        savingNow.current = false
        setSaving(false)
        setSaved(answer)
        if (made !== undefined && impactChanged(made.impact, answer.impact))
          setSaveMismatch(
            'The save differs from the preview it was made from. Something changed the library ' +
              'between the two — the numbers below are what actually happened.',
          )
        // O96: a commit the form made while this save was in flight is
        // applied *on top of* the answer rather than discarded by it.
        // `answer.rules` is positionally aligned to the list that was sent —
        // `rules::replace_all` preserves position and mints an id only for a
        // zero one — and `RuleList` was frozen for the duration, so `index`
        // still names the same rule. **`id` and `ordinal` come from the
        // answer** for that reason: carrying the edit's own `id: 0` forward
        // would mint a *second* row on the next save and orphan
        // `created_rule_id` on any accepted proposal that wrote the first,
        // reopening it for nothing — the very reopening `save_rules`' own doc
        // comment contrasts against `import_rules`.
        const pending = held.current
        held.current = undefined
        const next =
          pending === undefined
            ? answer.rules
            : answer.rules.map((rule, i) =>
                i === pending.index
                  ? { ...pending.rule, id: rule.id, ordinal: rule.ordinal }
                  : rule,
              )
        setRules(next)
        if (pending !== undefined)
          setHeldNotice(
            `Your edit to rule ${pending.index + 1} arrived while the save was running, so ` +
              'it is not in the save reported below — it is applied to the list on screen ' +
              'and is unsaved. Save again to apply it.',
          )
        // I1: the button's number has to describe *this* saved list, not the
        // one a save ago — so a fresh preview runs for it here, through the
        // same seq-guarded `runPreview` every other gesture uses, and Save
        // stays disabled (`busy`) until it lands. `selected` is not stale:
        // O96 froze `RuleList`, and `PreviewTabs`' two options have been
        // `disabled={busy || saving}` since O95, so no surface can change it
        // while a save is in flight. That is a property of *every* such
        // surface being guarded, not of this line — a new unguarded one
        // reopens the defect without touching `save`.
        runPreview(next, selected)
        // The list just changed under every accepted proposal's
        // `created_rule_id`, so the badges are re-read rather than assumed.
        acceptedProposals()
          .then(setProposals)
          .catch(() => {})
      })
      .catch((err: unknown) => {
        savingNow.current = false
        setSaving(false)
        const pending = held.current
        held.current = undefined
        if (pending === undefined) {
          // Unchanged: nothing follows this, so `failure` survives to be read.
          setFailure(message(err))
          return
        }
        // O96, spec §5.1: the save wrote nothing, so the list is still the
        // one that was sent and the held edit applies to it unchanged — no id
        // came back to adopt. The failure text goes to `heldNotice` rather
        // than `failure` because `commit` fires a preview whose success path
        // calls `setFailure(undefined)`, and the reason a person most needs
        // to read here is the 409 `save_rules` answers while a scan holds
        // `library_job`.
        setFailure(undefined)
        setHeldNotice(
          `The save failed: ${message(err)}. Your edit to rule ${pending.index + 1} ` +
            'arrived while it was running — it is applied to the list on screen and is ' +
            'unsaved.',
        )
        commit(
          rules.map((rule, i) => (i === pending.index ? pending.rule : rule)),
          selected,
        )
      })
```

- [ ] **Step 6: Clear the notice when a save starts, and draw it**

In `save`, beside the existing clears:

```tsx
    setFailure(undefined)
    setSaveMismatch(undefined)
    setHeldNotice(undefined)
```

In the returned JSX, immediately after the `saveMismatch` paragraph:

```tsx
      {heldNotice !== undefined && (
        <p role="alert" className="held-edit">
          {heldNotice}
        </p>
      )}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `pnpm --dir web test:jsdom RuleEditor`
Expected: PASS, all tests in the file.

- [ ] **Step 8: Hand-apply each defect and watch the right test fail**

Run four times, restoring between each:

1. In the merge, change `{ ...pending.rule, id: rule.id, ordinal: rule.ordinal }`
   to `pending.rule`. Expected: *keeps a form commit* fails on `id: 7` — and
   **only** it. If the notice test also fails, the notice test is coupled to
   the merge and must be narrowed.
2. In `onCommit`, delete the `if (savingNow.current)` branch. Expected: *keeps
   a form commit*, *says the held edit*, and *keeps a held edit when the save
   fails* all fail; the unchanged-path test still passes.
3. Change `setHeldNotice(...)` on the success path to `setFailure(...)`.
   Expected: *says the held edit is applied and unsaved* fails — this is the
   assertion that proves the slot, not just the sentence.
4. In the `.catch`, delete the `pending` branch entirely (restore today's
   `setFailure(message(err))` only). Expected: only *keeps a held edit when
   the save fails* fails.

- [ ] **Step 9: Update `RuleEditor.tsx`'s top doc comment**

Append two paragraphs before the closing ` */`:

```
 * **A save freezes the list and holds the form's commit — O96.** `save`'s
 * `.then` replaces the whole list with `answer.rules`, so every gesture
 * committed during the ~281 ms it takes used to be applied, previewed and
 * silently discarded. `RuleList` is frozen for the duration; `RuleForm` is
 * deliberately not, because `disabled` on a focused input makes the browser
 * fire `blur` and this form commits on blur — the mechanical guard would
 * itself commit the half-typed edit into the window it was meant to close.
 * So the form stays live and its commit is *held*, then applied on top of
 * `answer.rules` taking that row's `id` and `ordinal` from the server. The
 * freeze is what makes that merge exact: `answer.rules` is positionally
 * aligned to the list that was sent, and a list that moved under the request
 * has no correspondence to it at all.
 *
 * **`selected` in `save`'s closure is no longer stale, and that is a property
 * of the guards rather than of this file.** It used to be: selecting a rule
 * mid-save left the follow-up preview's `rows` scoped to the rule that *was*
 * open while the form showed the new one. Nothing can change `selected`
 * during a save now — `RuleList` is frozen and `PreviewTabs`' two options
 * have been `disabled={busy || saving}` since O95 — so a `selectedRef` would
 * hide the invariant rather than state it. A new surface that sets `selected`
 * without a guard reopens the defect without touching `save`.
```

- [ ] **Step 10: Commit**

```bash
git add web/src/RuleEditor.tsx web/src/RuleEditor.test.tsx
git commit -m "O96: a save holds the form's commit instead of discarding it

The form stays live — disabling a focused input fires blur, and this form
commits on blur, so the mechanical guard would commit the half-typed edit
into the window it was meant to close. Its commit is held and applied on
top of answer.rules, taking that row's id and ordinal from the server: the
edit's own id: 0 would mint a second row next save and reopen the proposal
that wrote the first. A failed save keeps the edit too, and states its
reason where the follow-up preview cannot wipe it.

Also closes the stale `selected` in save's closure, structurally: with the
list frozen and PreviewTabs guarded since O95, nothing can change it."
```

---

### Task 4: Close O96 in `docs/status.md` and run the gate

**Files:**
- Modify: `docs/status.md`

**Interfaces:**
- Consumes: the six tests added by Tasks 2 and 3.
- Produces: nothing.

- [ ] **Step 1: Run the sweep on the way in**

Run:
```bash
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```
Record the four answers before editing anything. The count word drifts on the
way in as well as out.

- [ ] **Step 2: Establish whether web tests move the counted figure**

Run: `grep -n 'tests' docs/status.md | head -20` and read what the count claims
to count. The `status-test-count` CI job compares it against a JUnit report.
If the figure is the cargo workspace's alone, Tasks 2 and 3 add **zero** to it
and steps 4's test-count edits are skipped; if it includes the web suite, it
moves by **six**. Do not guess — read the sentence and the job.

Run: `grep -rn 'status-test-count' .forgejo/ scripts/ | head`

- [ ] **Step 3: Check the web coverage floor**

Run: `pnpm --dir web test:coverage 2>&1 | tail -30`
Expected: passes. Six new tests raise coverage against a floor that is a
minimum, so this should not fail — but per-file floors in this repo drift, and
this is the cheapest place to find out.

- [ ] **Step 4: Make O96's five-part close**

1. The table row's checkbox: `| O96 | [ ] |` becomes `| O96 | [x] |`.
2. The row's prose, rewritten to open with what closed it. Draft:

```
**Closed 2026-09-06 by freezing the list and holding the form's commit, which are one decision and not two.** `RuleList` takes a `saving` prop and disables its five gestures — the select, the enable toggle, the drag-reorder, the add, the delete, and the confirm dialog's *Delete and reopen* — with **two** guards on the drag, because `draggable={!saving}` cannot cancel a drag already in progress when the save began. On `saving` and never on `busy`: a preview in flight is superseded by `runPreview`'s sequence guard and is never lost, so a gesture during one is correct and cheap, and gating on `busy` would grey the list for ~281 ms after every gesture. **`RuleForm` is deliberately not frozen**, and the mechanical guard is worse than it looks: `disabled` on a *focused* input makes the browser fire `blur`, and this form commits on blur — so disabling it would itself commit the half-typed edit into the very window this item is about. Instead the fields stay live and the *commit* is held, then applied on top of `answer.rules` taking that row's `id` and `ordinal` from the server. **The freeze is what makes that merge exact**: `answer.rules` is positionally aligned to the list that was sent, `rules::replace_all` minting an id only for a zero one, and a list that moved under the request has no correspondence to it at all — which is also why the third option, freezing nothing and reconciling afterwards, was refused: it needs a client-side row key `RuleList`'s own identity model does not have. Taking the edit's `id: 0` forward instead would mint a **second** row on the next save and orphan `created_rule_id` on any accepted proposal that wrote the first, reopening it for nothing; the fixture returns a minted `id: 7` against a sent `id: 0` precisely so a test cannot pass against that. **A second defect in the same window closed with it, structurally rather than by a fix**: `save`'s `.then` calls `runPreview(answer.rules, selected)` with the `selected` its own closure captured, so selecting a rule mid-save left the preview's `rows` scoped to the rule that *was* open while the form showed the new one — a sharper symptom than the discarded gesture, because nothing about it looks wrong. With `RuleList` frozen and `PreviewTabs`' two options `disabled={busy || saving}` since O95, no surface can change `selected` during a save; a `selectedRef` was refused because it would hide that invariant rather than state it. **A failed save keeps the edit too** — it wrote nothing, so the list is still the one that was sent — and its reason goes to the notice slot rather than to `failure`, because applying the edit runs a preview whose success path clears `failure` and the reason a person most needs is the 409 `save_rules` answers while a scan holds `library_job`. That is the third instance in one file of a notice needing a slot `runPreview` does not touch, after `importNotice` and `saveMismatch`. **What remains: nothing enforces that a future surface setting `selected` carries a guard.** The invariant is stated in `RuleEditor.tsx`'s doc comment and asserted nowhere
```

3. The "N are now closed — leaving M" sentence: increment closed, decrement open.
4. The "What remains … M open items" line — **read the next line too**, the
   count word wraps and a grep for the phrase finds nothing.
5. The test count at **both** sites, if and only if Step 2 said web tests count.

- [ ] **Step 5: Grep the bare O-number everywhere and read every hit**

Run: `grep -rn 'O96' docs/ web/src/ crates/ .forgejo/ 2>/dev/null`
Read every hit. Prose outside the table describes open items in wording no
sweep can predict — closing O25 left a paragraph four lines above *What
remains* still saying, in the present tense, that the thing was not fixed.

- [ ] **Step 6: Run the sweep on the way out**

Run the same four commands as Step 1 and check each answer against the edits.

- [ ] **Step 7: Run the full gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: the final line reads `all green`. **Quote that line in full** — piping
through `tail` or appending `; echo` masks the exit status.

Do **not** run it with `TAIYAKU_CORPUS` set: the script exports it into an
unfiltered `cargo test --workspace`, which runs `geometry_experiment` in debug
and has taken over an hour.

- [ ] **Step 8: Run the corpus checks separately**

Run: `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`
Expected: ~85 s, twelve standing checks, all passing.

- [ ] **Step 9: Commit**

```bash
git add docs/status.md
git commit -m "O96 closes: a save freezes the list and holds the form's commit

Five-part close per the open-items ritual. Records the second defect that
closed with it — save's closure captured `selected`, so selecting a rule
mid-save scoped the follow-up preview's rows to the rule that was open —
and what remains: nothing enforces that a future surface setting
`selected` carries a guard."
```

---

### Task 5: Drive it against the real library

The three whole-branch reviews on O95 each found defects the round before had
introduced, and by the third pass every one was in prose. Reading is not
verifying — this task is what tells the difference.

**Files:** none.

**Interfaces:**
- Consumes: everything above.
- Produces: a line in `docs/notes.md` if anything is measured.

- [ ] **Step 1: Serve a copy of the real library**

Follow the arrangement in
`/home/davey/.config/.claude/projects/-home-davey-projects-taiyaku/memory/taiyaku-verifying-against-real-material.md`
— a copy at `/home/davey/**temp**/manga-test`, writes isolated and proven
isolated afterwards.

- [ ] **Step 2: Watch the freeze**

Open `/config/rules`, start a save, and during the ~281 ms window try each
frozen gesture: drag a row, tick an enable box, click another rule, press Add,
press Delete. Expected: none of them takes.

- [ ] **Step 3: Watch a held edit survive**

Start a save, and while it is in flight edit the open rule's `pattern` and tab
away. Expected: the field keeps the edit, the notice names that rule, the
`Saved:` line is still on screen, and the Save button re-enables stating a
number that includes the edit.

- [ ] **Step 4: Watch a failed save keep it**

Start a scan, then save during it — `save_rules` answers 409. Expected: the
notice carries both the scan's reason and the edit's fate, and it does not
vanish when the follow-up preview lands.

- [ ] **Step 5: Record what was measured**

Anything timed goes in `docs/notes.md`, not `status.md` — status.md's preamble
promises every claim there traces to a measurement recorded in notes.

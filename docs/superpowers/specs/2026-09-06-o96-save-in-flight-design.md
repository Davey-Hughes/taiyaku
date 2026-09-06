# O96 — what a save does to work already in progress: build design

**Status: settled in brainstorming on 2026-09-06.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O96**, opened 2026-09-06 by the third whole-branch review of
O95's branch, which found it while checking whether the guard O95 added to two
buttons covered the screen. The item's entry in `docs/status.md` carries the
evidence; this document settles only what that entry left open, which is the
whole of why it was an item rather than a fix — *"What a save should do to work
already in progress is a design decision (queue it, refuse the save, or merge
the answer into the current list rather than replacing it), and nobody has
taken it."*

The product design is
[`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
(`C4 §1`) and the build design is
[`2026-09-02-library-configuration-build-design.md`](2026-09-02-library-configuration-build-design.md)
(`build §5`). Section references below name which.

## 0. The defect in one paragraph

`RuleEditor::save` sets `saving` and never `busy`, and its `.then` ends with
`setRules(answer.rules)` — so any gesture committed between the request and its
answer is overwritten by the list the server sent back, with nothing on screen
saying so. A save costs a median **280.7 ms** on the real library
(`docs/notes.md`, the `apply`-unchanged-list row of C4 PR B's own measurement),
which is an ordinary length of time to start a drag in. Save itself is guarded
(`saving || busy || preview === undefined`) and, since O95, so are the Overrides
row's two options — but `RuleList` and `RuleForm` take no `busy` or `saving`
prop at all and render no `disabled` anywhere, so the select, the enable toggle,
the drag-reorder, the add, the delete and the form's own commit-on-blur are all
live. The drag is the likeliest: a reorder is applied, previewed, and thrown
away.

**A second defect lives in the same window and this document closes it too.**
`save`'s `.then` calls `runPreview(answer.rules, selected)` with the `selected`
its own closure captured when the save started. Selecting a different rule
mid-save therefore leaves the preview's `rows` scoped to the rule that *was*
open while the form shows the new one — a sharper symptom than the discarded
gesture, because nothing about it looks wrong.

## 1. What ships: the list freezes, the form does not

**Decided by the owner 2026-09-06.** A save freezes `RuleList` and leaves
`RuleForm` fully live; a form commit that arrives during the flight is **held**
and applied on landing, onto the server's own row, rather than discarded.

The two halves are one decision and not two. §4 gives the reason: the merge in
§3 is exact only because the list did not move, so the freeze is what makes
keeping the edit safe.

**Why freezing the list is the right shape rather than a heavy one.** The
server already models a save as exclusive — `config_api::save_rules` tries
`ServeState::library_job` once and answers **409** if a scan or another save
holds it, never waiting. The screen has simply not matched the route it calls.

**Rejected: freeze nothing and merge the answer into whatever is on screen.**
The most permissive option and by a wide margin the most work. `RuleList`'s own
doc comment states the identity model — *"Everything here addresses a rule by
its position, never by its id"*, because two rules the editor has added and not
saved both carry `id: 0`. `answer.rules` is positionally aligned to the list
that was sent, so a reorder, an add or a delete during the flight destroys the
only correspondence there is. Reconciling across one would mean giving every row
a client-side key, which is a change to the model that comment states rather
than a guard on top of it.

**Rejected: freeze the whole screen, form included.** One prop threaded to both
children and the smallest diff of the three. It fails on the form for the reason
O96's entry names, and the failure is worse than it first looks: `disabled` on a
**focused** input makes the browser fire `blur`, and `RuleForm` commits on blur
— so the mechanical guard would itself commit the half-typed edit into exactly
the window this item is about. `readOnly` avoids that (it keeps focus and fires
no blur) at the cost of a person watching their keystrokes vanish with nothing
on screen explaining it.

## 2. `RuleList` — frozen on `saving`, and on nothing else

A new `saving: boolean` prop. `disabled={saving}` on the enable checkbox, the
select button, each row's Delete, the `+ Add` button, and the confirm dialog's
*Delete and reopen*.

`draggable={!saving}` **and** an early return in `onDrop`. Both, not either: a
drag begun before the save started is already in progress, and flipping
`draggable` to `false` under it does not cancel the drop that ends it.

**On `saving`, never on `busy`.** They gate different things, the same
distinction `PreviewTabs`' own prop pair already documents. A preview in flight
is superseded by `runPreview`'s `latest` sequence guard and is never lost, so a
gesture during one is correct and cheap; only a save overwrites the list.
Gating on `busy` would grey the list for ~281 ms after *every* gesture, which is
the screen's normal working rhythm — `build §5.3` costs that rhythm out and
refuses a debounce for the same reason.

### 2.1 Import is refused, not held

**Found by this branch's own whole-branch review, 2026-09-06; §§1–2 above were
written without it and named the form as the only surface left live.**

`RuleEditor`'s Import button and its hidden file input carried no `disabled` —
`disabled` appeared exactly once in the whole JSX, on Save — and `doImport`
commits like every other gesture.

**Import is not like the others, because `POST .../rules/import` writes the
rule table.** An import landing inside the save window left the *store* holding
the imported rules, the *screen* showing the pre-import list `setRules` put
back over them, and `importNotice` telling the person the import had worked.
The next Save would then write the screen's list back, reverting the import and
reopening every affected proposal a second time. Every other discarded gesture
costs a click; this one outlives the session.

**So it is refused, and that is forced rather than chosen.** The held-commit
answer of §3 cannot apply: by the time a held import could be merged, the write
has already happened. The guard has to sit *before* the request.

**Two guards, for §2's reason.** The button is `disabled` while saving, *and*
`doImport` checks `savingNow.current` before calling `importRules` — a file
picker opened before the save started is still open and still delivers. The
refusal says so, in `importNotice`, rather than dropping the file silently.

**Rejected: queue the import and send it after the save lands.** It would
apply a document chosen against one rule list to whatever the save produced,
and the reopened-proposals report would name proposals the person never saw
the state of. Refusing is the honest answer and the cheap one.

## 3. `RuleEditor` — one held commit

```ts
const held = useRef<{ index: number; rule: Rule } | undefined>(undefined)
```

`RuleForm`'s `onCommit` handler: if `savingNow.current`, store `{ index:
selected, rule: edited }` and return; otherwise `commit(...)` exactly as today.
The form's own local draft still shows the edit, so the screen is not lying
while the save is in flight — it is showing an edit that has not been previewed
yet, which is what an uncommitted field already looks like.

In `save().then`, after `setSaved(answer)`:

```ts
const next =
  pending === undefined
    ? answer.rules
    : answer.rules.map((r, i) =>
        i === pending.index
          ? { ...pending.rule, id: r.id, ordinal: r.ordinal }
          : r,
      )
```

**`id` and `ordinal` come from the answer and every other field from the edit.**
Taking the edit's own `id` would send `0` again on the next save —
`rules::replace_all` mints a fresh row for a zero id — which would create a
*second* row for a rule the person only edited, and orphan `created_rule_id`
on any
accepted proposal that wrote the first, reopening it for no reason. That is the
same reopening `config_api::save_rules`' doc comment contrasts against
`import_rules`, arriving through the back door.

**Two commits in one window are the same rule.** The list is frozen, so
`selected` cannot move; the later commit wins, which is correct.

## 4. The stale `selected` closes structurally, not with a ref

With `RuleList` frozen and `PreviewTabs`' two options already
`disabled={busy || saving}` since O95, no surface can change `selected` while a
save is in flight. `save`'s closure is therefore no longer stale, and
`runPreview(next, selected)` is correct as written.

This is worth a comment in the file rather than silence, because the property
is not local: it holds only for as long as *every* surface that can set
`selected` is guarded, and a future one that is not would reopen the defect
without touching `save` at all.

**Rejected: a `selectedRef` mirroring the state.** It works, it is three lines,
and it hides the invariant instead of stating it — a new unguarded surface would
then silently preview the right rule while still discarding the gesture, which
is a worse failure than a visible one.

## 5. The notice — its own state slot

```ts
const [heldNotice, setHeldNotice] = useState<string | undefined>()
```

Set on landing when a commit was held, cleared at the top of `save()` beside
`setSaveMismatch(undefined)`.

**Not `failure`.** The follow-up `runPreview` that `save` fires on landing
clears `failure` when it succeeds, so a notice stored there would be wiped by
the very call that populated it — the trap `importNotice` and `saveMismatch`
each already carry a doc comment about, and this is the third instance of it in
one file.

The notice names the rule and says the edit is applied and unsaved. It exists
so the `Saved:` report can stay on screen: `commit`'s own `setSaved(undefined)`
exists to stop a stale save report standing next to a list it does not describe,
and without a notice the choice would be between that stale pairing and throwing
away the save's `complaints` and `derive` report to a stray blur.

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

## 6. Tests

Five, and each is hand-applied against the defect it names before it is
believed — the standing rule this repository learned on C4 PR B, where eleven
tests passed against their own defect. Per-element assertions, never a
whole-page `toContain`.

1. `RuleList.test.tsx` — with `saving`, each of the five controls is `disabled`
   and the row is not `draggable`, asserted **per element**.
2. `RuleList.test.tsx` — a drop arriving while `saving` calls `onReorder` zero
   times. This is the half `draggable` alone does not cover.
3. `RuleEditor.test.tsx` — a form commit during a held-open save survives it,
   and the rule keeps the **server's** minted id. The fixture sends rule 2 as
   `id: 0` and returns it as `id: 7`; a test whose fixture returns the ids it
   sent cannot fail against §3's defect.
4. `RuleEditor.test.tsx` — the notice renders, names the rule, and is still on
   screen after the follow-up preview resolves.
5. `RuleEditor.test.tsx` — with no edit in the window, the landed list is
   `answer.rules` exactly. The regression guard on the unchanged path.

The held-open-save fixture already exists in `RuleEditor.test.tsx` (the
`resolveSave` pattern), so 3–5 extend a shape the file has rather than
introducing one.

## 7. What this does not do

**It does not queue *list* gestures.** A drag attempted during a save does not
happen and is not remembered; the row is simply not draggable. Holding a
reorder would mean holding it against a list whose ids are about to change,
which is §1's rejected third option wearing a smaller hat.

**It does not touch the 409.** Two administrators saving at once still get the
server's refusal, which is the right place for it. This is a single client's
own race with itself.

**It does not address the blur-then-click ordering on the Save button.** A
click on Save while a text field is focused fires `blur` first, which commits,
which sets `busy`, which disables the button before the click lands. That is a
separate window with a separate cause and is not measured; it is noted here so
the next reader does not take this document's silence for absence.

## 8. Documents this changes

- `docs/status.md` — O96's five-part close, per the open-items ritual: the
  checkbox, the prose bullet, the closed/leaving sentence, the *What remains*
  line, and the test count at **both** of its sites, since §6 adds five tests.
- `RuleEditor.tsx`'s top doc comment — the held commit, the notice's slot, and
  the `selected` invariant §4 states.
- `RuleList.tsx`'s top doc comment — that the freeze is what keeps its
  positional identity model valid across a save.

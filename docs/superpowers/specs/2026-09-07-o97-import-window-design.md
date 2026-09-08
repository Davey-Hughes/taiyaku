# O97 — one writer at a time: build design

**Status: settled in brainstorming on 2026-09-07.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O97**, opened 2026-09-06 by the whole-branch review of O96's own
branch, which found it while checking whether the freeze O96 added covered the
screen — and deliberately did not fix it there, because O96's claim is about
work committed *during a save* and this is a save started *during an import*.
The item's entry in `docs/status.md` carries the evidence; this document settles
what that entry left open: *"whether Save should be refused for the duration, or
the import should be, or the second one queued."*

It also closes two losses in the same window that carry no O-number, found by
reading the code while settling this one. §0.1 names them.

The product design is
[`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
(`C4 §1`) and the build design is
[`2026-09-02-library-configuration-build-design.md`](2026-09-02-library-configuration-build-design.md)
(`build §5`). Its direct predecessor is
[`2026-09-06-o96-save-in-flight-design.md`](2026-09-06-o96-save-in-flight-design.md),
whose reasoning this document reuses rather than restates wherever the two
windows are the same shape; section references below name which.

## 0. The defect in one paragraph

`doImport` never touches `busy` and sets no flag of its own — it calls
`importRules(text)` and commits on the answer — while Save's guard is
`saving || busy || preview === undefined`. So nothing stops a person pressing
Save between an import request and its answer. Both routes write the rule
table, and what follows is worse than a lost gesture: the save sends the
*pre-import* list, the import lands and `commit`s the imported one onto the
screen, then the save lands and `setRules` puts the pre-import list back over
it. The store now holds whatever wrote last, with `importNotice` still saying
the import worked — and the next Save writes the screen's list back, reverting
the import and reopening every proposal a second time. **Not introduced by
O96's branch**: Save has been enabled during an import since the Import button
shipped in #109. O96 guarded the other direction and this one became visible by
contrast.

### 0.1 Two more in the same window, neither numbered

Found by reading `doImport` while settling the above, and closed here because
they are the same defect with a different gesture rather than new ground.

**Every other gesture is discarded too.** `doImport`'s success path calls
`commit(answer.rules, undefined)`, which replaces `rules` wholesale — so the
enable toggle, the select, the delete, the drag-reorder, the add and the form's
own commit-on-blur are each applied to the screen during the import and then
wiped by the list the server sent back, with nothing saying so. This is O96's
defect exactly, with the import as the writer instead of the save, and O96's
own worst finding was the one surface nobody had enumerated. Cheaper than the
Save case — a click rather than a session — and left live it is what the next
whole-branch review finds.

**Import is enabled during an import.** Its button is `disabled={saving}` and
nothing else, and `doImport` checks only `savingNow.current`, so a second pick
delivered before the first answer arrives races two writes to the rule table
with no guard between them. Reachable with one file picker: the button is live
for the whole window.

### 0.2 The window's length is not measured

O96 could name its window — a save costs a median **280.7 ms** on the real
library (`docs/notes.md`), which is an ordinary length of time to start a drag
in. **The equivalent figure for an import does not exist**, and this document
does not invent one. `POST /api/library/rules/import` writes the rule table and
derives nothing, so it should be the cheaper of the two, but "should be" is a
guess and it is marked as one. The implementation measures it against a copy of
the real library, the same way O96's was, and the figure goes to
`docs/notes.md`. Nothing in this design depends on its value — a window that
must not admit a second writer is the same design at 5 ms as at 500 ms — so the
measurement is a claim the branch owes rather than an input it waits on.

## 1. What ships: one writer at a time, named

Both Save and Import write the rule table. Neither may run while the other is in
flight, and neither may run twice. The disposition when one is refused is
**refuse, do not queue**, and it is the same in both directions.

**Why refuse rather than queue.** O96 settled the identical question for Import
during a save and its reasoning transfers without change: `POST
.../rules/import` writes the rule table, so by the time a queued import could be
applied the write has already happened — there is no state in which holding it
is a smaller loss than refusing it. In the direction this item names, queueing
is worse still: a Save queued behind an import would send a list the import has
already replaced, which is the very sequence §0 describes.

**Why not refuse the import instead.** The third option O97's entry names. A
refused Save costs a click; a refused import costs a click. But the *unrefused*
cases are not symmetric — an import landing wrongly reopens accepted proposals
and outlives the session, while a save landing wrongly is one derivation. When
only one of two writers can proceed, the one already in flight is the one that
has already spent the work, and the one whose loss is bigger is the one worth
protecting.

## 2. `RuleEditor` — the gate is one variable, not four

Today's gate is two things that mean one thing: `savingNow`, a ref read
synchronously by `save` and by `doImport`, and `saving`, state read by the
render. Closing O97 by adding an `importing` pair beside them would make four
flags admitting a state that must never exist — `saving && importing` — and
four conditions that can disagree.

They become one variable in each register:

```ts
/** Which write is in flight, if any. The synchronous gate. */
const writer = useRef<'save' | 'import' | undefined>(undefined)
/** The same fact, for the render. */
const [writing, setWriting] = useState<'save' | 'import' | undefined>()
```

`saving` is derived (`writing === 'save'`) wherever the old state was read, and
the mutual exclusion is a property of the type rather than of a condition
somebody has to keep in sync.

**This is the shape #115 arrived at, and the reason is the same.** Three rounds
of patching one screen produced three rounds of findings and one refactor ended
it: when every fix moves one condition and the gates then disagree, the defect
is the absence of a single source of truth. `phaseOf` folded six variables into
eight named phases; this folds four flags into one three-valued one. The
alternative — two booleans and an invariant asserted in a comment — is the
shape O95's third pass kept finding, and O96 reproduced it on the branch that
cites it.

**Both writers open with the same line.** `save` and `doImport` each begin
`if (writer.current !== undefined)`, refuse, and name which writer holds it.
This *absorbs* O96's existing `savingNow.current` check in `doImport` rather
than adding beside it, and closing §0.1's second finding costs nothing further:
`writer.current === 'import'` refuses a second import through the line already
written for the first.

**Both guards stay, for the reason the drag has two.** The button's `disabled`
is state and lands on a render; the ref is read synchronously. Between
`setWriting('import')` and the DOM update there is a real window in which a
click reaches an enabled button — which is precisely the window `savingNow`
exists for on the other side, and the asymmetry between the two is what the
tests in §6 pin.

## 3. The frozen surfaces — `saving` is renamed, not overloaded

`RuleList` takes a `saving: boolean` prop and reads it in seven places — the
enable toggle, the select, the delete, the confirm dialog's *Delete and
reopen*, the add, `draggable`, and the `onDrop` guard. `PreviewTabs` takes one
too, and its two override options are `disabled={busy || saving}`.

**Both props are renamed `frozen` and passed `writing !== undefined`.** Nothing
else in either file changes: every control already keys off that one prop, so
the freeze O96 built for the save window covers the import window the moment it
is handed the wider fact.

**Renamed rather than passed a lie under the old name.** A prop called `saving`
that is true during an import is a name that no longer describes its value, and
a symbol whose name has stopped being true is the staleness this repository's
open-items table exists to avoid — O96's second finding was a comment asserting
a guard that did not exist, which is the same failure a rank lower. The rename
touches the two components' prop types, their call sites and their tests, and
nothing else.

`RuleList`'s own doc comment currently says **"`saving`, never `busy`"** and
explains why a preview in flight is superseded while a save is not. That
reasoning is unchanged and survives the rename: an import, like a save, is a
write whose answer replaces the list, and `busy` is still the thing that must
not freeze anything.

**Save's `disabled` becomes `frozen || busy || preview === undefined`** and
**Import's becomes `frozen`**, where `frozen` is the same `writing !==
undefined`. Import's is a widening — it was `saving` — and it is what makes
§0.1's second finding unreachable by the button.

**Export is deliberately not frozen.** It serialises `rules` — the list on
screen — into a `data:` URL on every render, writes nothing and reads no server
state. Exporting the pre-import list during an import returns exactly what the
person is looking at, which is the truth.

## 4. The form stays live, and its commit's disposition splits

O96's reason for not freezing `RuleForm` is unchanged and is not restated here:
`disabled` on a *focused* input makes the browser fire `blur`, and this form
commits on blur, so the mechanical guard would itself commit the half-typed
edit into the very window the item is about. The field stays live and the
*commit* is what the gate acts on.

What changes is that the gate now has two answers rather than one:

- **`writer.current === 'save'` → held**, exactly as O96 shipped it: recorded
  in `held.current` and applied on top of `answer.rules`, taking that row's
  `id` and `ordinal` from the server. Unchanged, and §6 keeps a test on it so
  this branch cannot quietly cost it.
- **`writer.current === 'import'` → discarded**, with a notice naming the rule
  and saying it was not applied.

**Discarding is forced rather than chosen.** Holding works for a save because
`answer.rules` is positionally aligned to the list that was sent — O96's freeze
is what makes that merge exact. An import's answer is aligned to nothing: it
replaces every rule and mints fresh ids, so index 4 of the answer is not the
rule the person was editing and applying the edit there would overwrite an
unrelated rule with it. There is no correspondence to carry the edit across,
which is why the same gesture has two dispositions rather than one.

## 5. The notice — a third slot, and why neither existing one works

The refusals and the discard need somewhere to be said, and neither slot
already on this screen can hold them.

**Not `heldNotice`.** `commit` clears it, by design and with its own comment
saying why — the notice names a rule by position and any mutation makes that
false. The import's success path calls `commit(answer.rules, undefined)`, so a
refusal written there is wiped by the very event the person needs it to
outlive.

**Not `importNotice`.** `doImport` clears it up front and its success path
overwrites it with the *"the imported rules are stored…"* sentence. A refused
Save written there survives until the import answers and then disappears,
leaving the person told the import worked and never told their Save did
nothing.

So: a third `string | undefined` state, **`refused`**, for a gesture refused or
discarded because a write was in flight. It is **not cleared by `commit`** —
that is the whole reason it exists — and is cleared when a write actually
starts, which is the moment its claim stops being true. This is the fourth
instance in this file of a notice needing a slot `runPreview` does not touch,
after `importNotice`, `saveMismatch` and `heldNotice`, and the pattern is now
worth naming rather than rediscovering: a message about a gesture that did
*not* happen cannot live in a slot owned by the gesture that did.

## 6. Tests

Each is hand-applied against its own defect and must fail alone; a test that
passes against the defect it names is what this repository keeps finding, and
reading is not verification. Assertions are **per element** — a whole-page
`toContain` cannot see a pairing, and cannot tell a disabled Save from a
disabled Import.

1. **Save is disabled while an import is in flight.** Asserts on the Save
   button's own `disabled`, with the import's promise held open.
2. **`save` refuses when `writer.current === 'import'`.** Drives the handler
   past the disabled button and asserts no `saveRules` call and a `refused`
   notice. Asymmetric to (1) by construction: this is what proves the two
   guards are not one test twice, the same way O96's `draggable`/`onDrop`
   split did.
3. **A second import is refused during an import** — §0.1's second finding, at
   the button and again at the handler.
4. **A form commit during an import is discarded, not held**, and the notice
   names the rule. Fails if the disposition falls through to O96's held path.
5. **A form commit during a save is still held** — the regression this branch
   could cost, kept because §4 changes the code that decides.
6. **`RuleList` and `PreviewTabs` are frozen during an import.** Per control,
   not per page.
7. **`refused` survives the import's `commit`.** Fails if the notice is moved
   into `heldNotice` or `importNotice`, which is §5's whole argument executed
   rather than asserted.

**`tsc` is part of the gate, not a formality.** Renaming a required prop leaves
every existing mount site of `RuleList` and `PreviewTabs` uncompilable, and
O96 shipped a file whose 21 tests all passed against a call that would not
build — vitest does not typecheck. `scripts/check-all.sh` is what catches it.

## 7. What this does not do

- **It does not queue anything.** §1 says why, in both directions.
- **It does not measure the import window before shipping.** §0.2: the design
  is the same at any length, and the figure is a claim the branch owes
  `docs/notes.md` rather than an input. *(This stopped being true before
  merge — the figure was measured on 2026-09-08 and is in `docs/notes.md`.
  See the postscript.)*
- **It does not enforce that a future writer takes the gate.** A third route
  that writes the rule table would have to set `writer` itself, and nothing
  makes it. This is O96's own remaining gap restated one level up — its entry
  says *"nothing enforces that a future surface setting `selected` carries a
  guard"* — and it is not closed here. The invariant is stated in
  `RuleEditor.tsx`'s doc comment and asserted nowhere.
- **It does not touch the server.** Both routes already serialise writes; this
  is a client that stops issuing a request it knows is wrong.

## 8. Documents this changes

- `docs/status.md` — O97's row and its prose bullet, the closed count, the
  "leaving N" sentence and the "What remains" line, per the five-part edit.
  §0.1's two findings close *with* it rather than opening rows of their own:
  they are the same defect and the same fix, and a row that opens and closes
  in one commit records nothing the entry does not.
- `docs/notes.md` — the import window's measured length (§0.2), and the drive
  against a copy of the real library.
- `RuleEditor.tsx`'s doc comment — O96's paragraphs on the held commit and on
  Import being refused are rewritten to describe one gate with two writers,
  not two guards that happen to agree.

## Postscript — what execution changed, 2026-09-08

Kept rather than rewritten, in this repository's habit: the sections above are
what was settled at brainstorm time, and this says where shipping departed
from them. `docs/status.md`'s O97 row is the authority on what actually
shipped.

**Two more defects, both the same shape as each other and neither in §0.1** —
a gate taken and never released. `doImport`'s `.catch` did not clear it, so a
failed import disabled both writers for the session; and `importRules` is not
`async` and evaluates `JSON.parse(doc)` as an argument, so a document that is
not JSON throws *synchronously*, before any promise exists for that `.catch`
to attach to. The second was found by the whole-branch review **after** the
first was fixed, one sentence away in the same function, which is the argument
for grepping a finding's shape rather than its function.

**Nine tests, not §6's seven.** The two extra are one per defect above.

**§0.2's figure was measured before merge rather than after**: a rule import
costs a median 2.3 ms whole-process against a save's 280.7 ms, ten runs
against an isolated copy of the real library (`docs/notes.md`). It is the CLI
store path, so the HTTP route's auth, parse and network are excluded, and the
NAS figure is still owed. It changed nothing about the design, which is what
§0.2 predicted.

**§6's plan for the two guards' asymmetry survived, and the reason sharpened.**
The test that drives a click past the disabled button pins
`saveButton.disabled === false` at click time, so the refusal is demonstrably
the ref's work and not the attribute's — measured in the fixture rather than
argued.

**Two defects in the *plan* built from this design**, both found by executing
it: it declared `frozen` a task before its first reader, which `noUnusedLocals`
rejects; and its discard test opened a rule *during* an import, which §3's own
freeze makes impossible.

**§7's last bullet still holds**: nothing enforces that a future writer takes
the gate.

# The library's curation UI — design

**Status: settled in brainstorming on 2026-08-14. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **spec C3** of the roadmap in `2026-08-14-library-store-design.md`, §10.
It depends on **C1**. Designed against live mockups, kept in
`.superpowers/brainstorm/` and gitignored; the decisions they settled are here.

## Where it came from

Spec C was four UIs. C1 took browse. What remained was split **by frequency**
rather than by subject:

- **C3, this one — curation.** Answering proposals and correcting metadata.
  Done constantly, from inside the library, next to the thing you are looking
  at.
- **C4 — configuration.** Roots, scan control, the rule editor with its
  per-field provenance display, and user management. Touched rarely, from a
  settings area.

**And a gap the split exposed: nothing owned the proposals screen.** Spec B
raises proposals and sets `state: open | accepted | dismissed`; no spec answered
them. That is not a cosmetic omission — proposals are how the 37 suffixed
directories in §1 of that spec get resolved and how a 146-file chapter edition
gets attached. Without this screen, spec B produces a table nobody can answer
and the library never groups its editions, which is the entire point of the
project.

## 1. The proposals inbox

**A list, with each row expandable to full evidence.** Chosen over two
alternatives, both of which the mockups made concrete:

- **Triage, one at a time** — a full-screen question with both covers large and
  `Y`/`N`/`S` keys. Fast for a backlog, and it loses the overview: you cannot
  see that 29 of 37 are the same kind of question, so you cannot spot the
  pattern and answer it once with a rule instead.
- **Inline only, no queue** — proposals appear on the series page they concern.
  Rejected on a fatal cost: **a proposal about a series you never visit is never
  answered.** With about 90% of the library single-edition (spec B §1), the
  pages carrying proposals are precisely the pages there is least reason to
  open.

The inbox keeps the overview and buys triage-grade evidence by expanding a row.
Collapsed rows are one line; one row is open at a time.

Filterable by `kind` and by score. Bulk selection for the obvious cases.

## 2. What the expanded row shows, and the two decisions inside it

Per side — the subject and the proposed target:

- **A cover and two interior pages.**
- **The full path on disk**, verbatim.
- Volume count, page count of volume 1, container format, root.
- The volume list, so a mismatch (11 against 14) is visible rather than
  summarised.

Then the scored reasons, each shown as its own line with its sign, and a plain
statement of what accepting would do.

**Decision one: interior pages, not just covers.** A colour edition's *cover* is
a different image from the mono edition's — different palette, sometimes
different art entirely. An interior page is the same panel layout in both.
Showing only covers would make the true case look *false* exactly where the
judgement is hardest, which is the opposite of what the evidence is for.

**Decision two: the panel states that nothing has compared the artwork.** Every
signal spec B scores is about **names and counts** — a suffix matching a
vocabulary, a prefix existing, volume counts agreeing. This project already owns
a better instrument: `signature.rs` and `taiyaku duplicates --against` answer
"are these the same volume" *from content*, and that is the measured mechanism
the suffix heuristic is a poor substitute for.

**It is not wired in, and C3 is deliberately not blocked on it.** Fingerprinting
two volumes runs to tens of seconds, and the code lives in `taiyaku-cli`, which
depends on `taiyaku-serve` — so calling it from the server needs project 2's
crate extraction (spec D) first. What ships is the **slot and the honest label**:
a `Compare v01` control that says plainly that nothing has measured the artwork.
An absent number that looks like a low score would be worse than a button that
admits it has not run.

## 3. Accepting writes a rule

Amended into spec B as §6.1 while designing this screen, because the constraint
comes from here and the mechanism lives there.

Accept does **not** write to `series` and `edition` directly. It writes a
**narrow `rule`** — a `prefix` match on that exact directory — and "accept every
case like this" is the same mechanism with a broader `regex`. That keeps spec B
§2's claim true: derivation is `scan_entry` + `rule` → structure, with one
input and nothing else feeding it.

**Accept is therefore a form, not a click.** "Yes, Full Color Edition is an
edition of Go-Toubun" leaves three things undecided — the edition key
(`en-color`), its kind (`official`) and its language — and the form pre-fills
them from the vocabulary that scored the proposal.

**The form shows the rule it is about to write, defaulted narrow.** Widening it
in place is what turns *"resolves 6 more"* into a preview rather than a
surprise, and it is the moment a person can answer 37 questions once instead of
37 times.

**Un-accepting is why `proposal.created_rule_id` exists.** The proposal row
remembers the rule it wrote, so undo deletes that rule and reopens the proposal.
Without it, an accept made before **C4** ships the rule editor would be a rule
its author can neither see nor remove — and C3 landing first makes that the
normal case rather than an edge one.

## 4. Dismissing

Per proposal, and **not** symmetric with accepting. Doing nothing to a proposal
already has an outcome — the directory stays its own series — so accepting
*changes* that outcome while dismissing *confirms* it. A dismissal feeds no
derivation and writes no rule; it is a `state` change.

The pattern-level control is a **score threshold**, not a suppression list.
Spec B §6's amendment records why: there is no pattern covering the false
positives in that spec's survey — `- Kanna no Nichijo`, `- Lost Girls`,
`- Gaiden`, `- Kurapika's Memories` are unrelated proper nouns whose only shared
property is scoring low. One number, with weak proposals hidden but reachable
behind a filter, is the shape that fits.

Accepted and dismissed proposals stay listed under their own filters. A
dismissal you disagree with later is a thing you have to be able to find.

## 5. Metadata editing

**Contextual, from the thing being corrected**, with the same editor reachable
in both places: a series page edits series-level fields; a volume's detail panel
edits work-level ones. There is no separate "metadata section" to navigate to,
because the correction is always prompted by looking at something wrong.

What is editable, all of it defined by the store spec's §3 and §4.4:

| Level | Fields |
| --- | --- |
| Series | titles and their `kind`, which is primary, reading direction, tags, `external_id` per provider |
| Work | volume number, title, series membership |
| Edition | label, `kind`, `language`, credits including the scanlation group |
| Member | `chapter_declared`, and `chapter_source` becomes `hand` |

**Every edit writes `<stem>.library.json` as well as the database**, per the
store spec §4.4 — that is the whole reason that file exists, and an editor that
skipped it would make losing the database cost the organisational work rather
than a rescan.

**`chapter_source` becoming `hand` is not bookkeeping.** `chapter_at` refuses to
infer a chapter number from a filename because two corpus series have declared
names that are wrong (`core/sidecar.rs:477`). A hand-set chapter number is a
different kind of claim from a parsed one, and AniList sync (spec G) will need
to know which it has.

## 6. The one feature that writes to your library

The ComicInfo write-back, whose gates the store spec designs in §13.2. C3 owns
only the confirmation, and the confirmation is the feature:

- It names the exact files it will rewrite, and their total size — *"9 archives,
  4.1 GB"* — before anything starts, because on this corpus an archival volume
  runs 430 MB per page and a zip cannot be edited in place, so this is a full
  copy and rename.
- It states the consequence in the terms that actually bite: **the `sha256`
  changes, which invalidates the sidecar's staleness check, regenerates every
  thumbnail of that member, and changes the work's `content_key`.**
- It is per work, never bulk, and never a side effect of saving metadata.
- It is absent, not merely disabled, for `rar` and `7z` members, since only zip
  is written.

An earlier instinct was to make this a checkbox on the metadata editor. It is
not: everything else in §5 is reversible and writes only files taiyaku owns.
This one rewrites the user's archives, and putting it on the same Save button
would make the irreversible thing a side effect of the reversible one.

## 7. The API

```
GET    /api/library/proposals?state=&kind=&min_score=
GET    /api/library/proposals/{id}          the expanded evidence
POST   /api/library/proposals/{id}/accept   body: the rule to write
POST   /api/library/proposals/{id}/dismiss
DELETE /api/library/proposals/{id}/accept   un-accept: drop the rule, reopen

PATCH  /api/library/series/{id}             metadata
PATCH  /api/library/works/{slug}
PATCH  /api/library/editions/{id}
PATCH  /api/library/members/{id}

POST   /api/library/works/{slug}/stamp-comicinfo    §6, guarded
```

`accept` taking the rule in the body rather than deriving it server-side is what
makes §3's preview honest: the client shows the rule, the server writes what it
was shown. A server that re-derived the rule could write something the preview
did not say.

Everything is admin-gated per A2. `stamp-comicinfo` wants its own capability
rather than riding on the admin role — it is the only route in the product that
modifies files the user owns, and a capability is what lets an administrator
exist who cannot do it.

## 8. Testing

- **Accept writes a rule and re-derivation reproduces the same structure**, which
  is spec B §6.1's constraint expressed as a test rather than a paragraph. This
  is the important one: it fails if anyone ever makes accept a direct table
  write.
- **Un-accept removes the rule and reopens the proposal**, with the library back
  to its pre-accept structure.
- **A dismissed proposal survives a rescan that adds a volume to its subject** —
  the `subject_id` keying decision from spec B §6, which is invisible until
  exactly this case.
- The threshold hides weak proposals and the filter reveals them.
- A metadata edit writes both the database and `<stem>.library.json`, asserted
  on the file rather than on the call.
- The ComicInfo confirmation names the file count and total size, and refuses a
  `rar` member.
- Every evidence field in §2 renders when present and degrades when absent —
  a proposal whose subject has no interior pages yet must not render a broken
  panel.

All of it lands in the `jsdom` project, per C1 §9: nothing here touches the
resolver, so the coverage this adds is coverage CI enforces.

> **Corrected 2026-08-26 by what shipped.** Not all of it did. Vitest's three
> projects split on *filename*: `curation.test.ts` is a `.test.ts` and so runs
> in **`node`**, and eight tests landed in **`browser`** (five in
> `AcceptForm.browser.test.tsx`, three in `Proposals.browser.test.tsx`) for
> the layout and focus questions jsdom cannot answer. The clause after the
> colon still holds for `node` and `jsdom`, which are both in CI's gate; the
> eight browser tests are the part outside it. C3's build design §6 carries
> the same sentence and the same correction.

## 9. What is deliberately not done

- **No roots, no scan control, no rule editor, no user management.** Spec C4.
- **No content comparison.** §2 ships the slot; spec D makes it reachable.
- **No bulk metadata editing** across many series at once. Komga has it; nothing
  here has produced the case for it, and a bulk editor over a model this young
  is a way to make 127 mistakes at once.
- **No alignment started from a proposal.** `kind: 'align'` proposals are listed
  and not actionable until project 2.
- **No undo beyond un-accept.** Metadata edits are corrected by editing again;
  the round trip to `<stem>.library.json` is the durability story, not a history.

## 10. Branch shape

1. The proposals API and the store queries behind it.
2. The inbox: list, filters, collapsed rows.
3. The expanded row and its evidence, including the unfilled `Compare` slot.
4. Accept as a form that writes a rule; un-accept; the rule preview.
5. Dismiss and the score threshold.
6. Metadata editing, and the `<stem>.library.json` write.
7. The ComicInfo confirmation, behind its own capability.

## 11. The measurements this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs`
- `docs/status.md` — the `web/` row, the test counts
- `docs/sidecar.md` — `<stem>.library.json` now has a second writer, and the
  note added for the store spec should say so

## 12. What could not be measured

- **Whether 37 proposals is a session or an afternoon.** The inbox was chosen
  partly on the belief that a scannable list beats a fast triage loop at this
  size. That is a judgement about a backlog nobody has worked through, and if it
  is wrong the fix is a triage mode over the same data rather than a redesign.
- **What the scoring threshold should default to.** Spec B's survey gives a
  hand classification of 37 proposals, not a distribution of scores — the
  scores do not exist yet. The default has to be set after a real scan produces
  them, and setting it before would be picking a number to fit a table that was
  never computed.
- **How long a ComicInfo stamp actually takes** on a large volume over the
  deployment host's storage. §6 promises to show a size; showing a *duration*
  needs a measurement nobody has taken.

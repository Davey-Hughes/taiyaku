# Building the library's curation UI — design

**Status: settled in brainstorming on 2026-08-26. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is the **build design for spec C3**, whose product design is
[`2026-08-14-library-curation-design.md`](2026-08-14-library-curation-design.md)
and is not restated here. That document settled *what* the screens are, against
live mockups which survive in `.superpowers/brainstorm/42470-1786918333/content/`
— `proposals.html` and `proposal-detail.html` — and are the authority on layout.
This one settles *what gets written*, in what order, and records the four
places where building it corrected a settled spec — three in C3 (§4) and one in
C1 (§8), the last of them a cost claim that a query plan disproves.

## 0. What already exists, which is more than the spec assumes

`taiyaku-library/src/proposals.rs` is 937 lines with 1423 lines of tests.
`raise`, `open`, `accept`, `unaccept` and `dismiss` all ship and are exercised,
and `taiyaku library proposals [--min-score N] [--accept ID] [--dismiss ID]
[--unaccept ID]` is a working CLI surface over them. `rule` carries
`set_edition_key`, `set_kind`, `set_language` and `set_series` columns.
`rules::Engine` resolves a `rel_path` against a rule list.

What does not exist: **any HTTP route**. `routes.rs` registers seven
`/api/library/*` paths and none of them is a proposal. Nor does any query
assemble §2's evidence, and nor does any screen.

So C3's step 1 is smaller than §10 implies and its steps 2–5 are not.

## 1. Scope: two PRs, and what is out

**PR 1 — the proposals inbox.** Spec C3 §§1–4, its §10 steps 1–5 complete,
*including* the rule widening.

**PR 2 — metadata editing, and O62.** Spec C3 §5, its §10 step 6, plus the
missing-files state C1 §8 asks for and C1 §5.5 deferred here. One claim, not
two: **the library stops asserting things that are not true** — a work titled
after one of its five editions (O67), and a work offered to a reader that
cannot be opened (O62). O44 and O53 are the mechanism for the first; §8 below
is the mechanism for the second, and it needs nothing from the proposals half.

The alternative was a third PR of its own. Rejected because O62 is one query,
one response field and one piece of UI treatment — small enough that a branch
carrying only it would be a branch carrying less than its own review costs.

**Out: the ComicInfo write-back** (C3 §6, step 7). It depends on nothing else
in C3 and is the least-used screen in it; it gets its own branch, keeping this
one to a claim that can be stated in a sentence.

The widening was the scope question, because it is the one part of PR 1 with no
store layer beneath it. It stays in because §3 calls it *"the moment a person
can answer 37 questions once instead of 37 times"*, and because §2 turns out to
make it nearly free — see the next section.

## 2. `accept` splits in three, and the guards survive

C3 §7 says accept takes the rule in the request body, so that *"the client
shows the rule, the server writes what it was shown"*. `proposals::accept(conn,
id)` derives the pattern itself, and its own doc records two **measured** bugs
that is the fix for: a title-derived pattern that matched nothing and left a
proposal `accepted` forever with the library untouched, and a `Publisher/`
pattern that silently moved an unrelated third series' volumes into the
accepted series' page sequence.

Three alternatives were considered. *Preview only, accept unchanged by id* — the
form could display the rule but not widen it, deferring §3's bulk case to C4's
rule editor. *Literal §7* — the server writes the body and the collateral guard
becomes a warning, which reopens the second measured bug by design. Rejected in
favour of the split below, which keeps both.

```
preview(conn, id)                    -> ProposedRule
resolves(conn, id, &ProposedRule)    -> Resolution
accept_with(conn, id, &ProposedRule) -> rule_id
```

- **`preview`** is everything `accept` currently does before it writes: the
  state guard, the `attach-chapters` refusal by name, the missing-`target_id`
  refusal, `common_directory_pattern`, the `"{target} - "` strip and slugify,
  and both of that path's own refusals (a renamed target, an empty slug). It
  returns the narrow `prefix` rule the spec calls the default, plus the two
  columns `accept` never filled — see §4.
- **`resolves`** is the dry run, built on `rules::Engine` rather than on a
  second matcher: load the live rules, add the candidate, re-resolve
  `scan_entry`, diff the series assignments. Returns
  `{ also_resolves: Vec<proposal_id>, collateral: Vec<series_title>,
  works_moved: usize }`.
- **`accept_with`** keeps I5's single transaction, keeps the ordinal choice
  (above the seeded four and above every earlier accept), and writes what it
  was handed.

**The guard and the counter are one query, which is why the widening is cheap.**
`other_series_under_prefix` refuses an accept whose pattern reaches series it
should not. A widened rule reaches more *on purpose* — but only ever the
subjects of other open proposals of the same shape. So the legitimate set is
`{subject, target} ∪ {subjects of proposals this rule also resolves}`, and
everything outside it is collateral. `Resolution` is that one computation read
two ways: `also_resolves` is the mockup's *"resolves 6 more"*, `collateral` is
the measured `Publisher/` refusal still firing. Generalising the guard is what
extends it to `regex` patterns, which it has never seen.

**Accepting a widened rule accepts every proposal it resolves**, each recording
the same `created_rule_id`. `unaccept` therefore has to reopen all of them:
today it deletes the rule and reopens only the row it was called on, which
under a widened rule would strand the other six as `accepted` pointing at a
rule that no longer exists — the precise state `created_rule_id` was added to
prevent, reached by a road that did not exist when it was written.

Two smaller store changes: `open()` generalises to `list(conn, {state, kind,
min_score})` for C3 §7's query string, and a counts-by-kind query feeds the
mockup's filter tabs (`All 37 / Edition 29 / Chapters 6 / Series 2`).

## 3. The evidence query

`evidence(conn, id) -> Evidence`, returning per side — subject and target:

| Field | Source |
| --- | --- |
| `path` | the common directory, verbatim, as §2 requires |
| `volume_count`, `volumes: Vec<f64>` | `work.volume` for the series |
| `root_name` | `root` via `member.root_id` |
| `first_volume: Option<{work_slug, edition_key, page_count, format}>` | volume 1's hub edition |

`first_volume` carries a slug and an edition key rather than image bytes,
because `/api/works/{work}/{edition}/pages/{index}` and its `/thumb` sibling
already exist — the evidence panel builds URLs against routes C1 shipped rather
than adding an image route to a curation module.

**The interior page indices are fractions of the page count, not fixed
numbers.** The mockup draws `v01 p012` and `v01 p047`, which are illustrative;
hard-coding them renders a broken panel for a three-page member, and C3 §8
requires every evidence field to degrade when absent. Deterministic fractions
give the same two pages for the same member every time, which is what makes the
side-by-side comparison meaningful at all.

## 4. Three corrections to the settled spec

Recorded here rather than applied quietly, because the reasoning is why each
was wrong.

**C3 §12 says the score threshold default *"has to be set after a real scan
produces them"*, and that is not so.** `SIGNAL = 1.0` and `CERTAIN = 2.0 *
SIGNAL + 1.0` make the scale `{0, 1, 2, 3}` — `edition-of` scores 0, 1 or 2 by
adding one per signal, and `same-series` and `attach-chapters` are both 3.
There is no distribution to survey: the default is a choice among four values,
decidable by reading `raise_edition_of`. Score 0 is *"the suffix matches no
edition word, and the volume counts differ"*, which is exactly the
`- Kanna no Nichijo` false positive C3 §4 names, and score-0 proposals are
inserted — `insert_proposal` is called unconditionally. §12's paragraph is
answered, not deferred.

**The mockup's accept form pre-fills `language en`, and it should not.** That
is a guess from an English-looking suffix, and `serve/src/comicinfo.rs` already
refuses the identical reasoning in its own header: *"deriving a language from
`raw` or `en` would be a guess presented as metadata, which is what
`work.root_id` was rejected for in spec C1."* `set_language` defaults empty and
the form offers it.

**`set_kind` is derivable and should be pre-filled, which the spec does not
say.** `EDITION_WORDS` has eight entries; `Fan Translated` is `fan` and the
other seven are `official`. The matched vocabulary word *is* the signal that
raised the proposal, so reading a kind off it asserts nothing the score does
not already rest on. This is the one of the three that adds a default rather
than removing one.

## 5. The server

A new module, `taiyaku-serve/src/curation.rs`. Not folded into `library_api.rs`,
which is C1's read layer over `browse.rs` and stays that.

```
GET    /api/library/proposals?state=&kind=&min_score=
GET    /api/library/proposals/{id}              evidence + the previewed rule
POST   /api/library/proposals/{id}/resolution   body: a rule -> what it would resolve
POST   /api/library/proposals/{id}/accept       body: the rule
DELETE /api/library/proposals/{id}/accept       un-accept
POST   /api/library/proposals/{id}/dismiss
```

`/resolution` is not in C3 §7 and is what makes the widening form honest: you
edit the pattern, and it says *"resolves 6 more"* or names the collateral,
before you commit. A `POST` that mutates nothing, because it takes a rule in a
body; named for what it returns rather than for a `dry_run` flag on `accept`,
since a flag that turns a write into a read is a route with two contracts.

Every one of the six is admin-gated per A2, and every one gets a row in
`routes.rs`'s grant table — the enumeration test that caught six drifted routes
in #85, and which is the reason to add all six in one edit rather than as each
handler lands.

## 6. The web

Following C1's shape: an API-client-and-pure-helpers module with its own test
file, screens as focused components.

- **`curation.ts`** — the six calls, their types, and the pure parts worth
  testing away from the DOM: score → `strong`/`weak` (strong at 2 and above, so
  a two-signal `edition-of` match ranks with the two kinds §0's `CERTAIN`
  places at 3), the tab counts, and the consequence sentence the mockup states (*"add edition `en-color` · kind
  `official` to series …, move 9 works, and keep every volume number as-is.
  Nothing on disk is touched."*).
- **`Proposals.tsx`** — the screen at `/proposals`: header count, filter tabs,
  the list, one row expanded at a time.
- **`ProposalRow.tsx`** — the collapsed line and the expanded evidence: two side
  panels, the signed reason list, the volume chips with missing ones dashed,
  and the `Compare v01` slot carrying C3 §2's honest label that nothing has
  compared the artwork.
- **`AcceptForm.tsx`** — edition key, kind and language; the rule shown
  defaulted narrow; the widening field calling `/resolution` on each edit.

The sidebar gains a `Proposals` entry with the open count, rendered only when
`useGate()?.who.admin` — the bit `Shell.tsx` already reads for the roots list,
for the same reason recorded there: only the gate knows who is asking.

All of it lands in the `jsdom` project per C1 §9.

> **Corrected 2026-08-26 by what shipped, and kept above because §4 records
> corrections rather than applying them quietly.** It did not all land there.
> Vitest's three projects split on *filename*, not on subject matter:
> `src/**/*.test.ts` is `node`, `src/**/*.test.tsx` is `jsdom`, and
> `src/**/*.browser.test.tsx` is `browser`. So the very first bullet above —
> `curation.ts`'s "pure parts worth testing away from the DOM" — lands in
> **`node`** by construction, along with the two other `.test.ts` files this
> branch added (`api.test.ts`, `index.css.test.ts`); and **eight tests landed
> in `browser`**, five in `AcceptForm.browser.test.tsx` and three in
> `Proposals.browser.test.tsx`, for the layout and focus questions jsdom has
> no engine to answer.
>
> **The consequence the sentence was actually making is unharmed**, which is
> why this is a correction and not a defect: `node` and `jsdom` are both in
> CI's gate, so their coverage is coverage CI enforces. Only the eight browser
> tests sit outside it, and `vite.config.ts` records that trade-off on the
> `jsdom` project itself. The product design's §8 carries the identical
> sentence and the identical correction.

## 7. PR 2: metadata editing, and a gap in the document

`librarydoc::apply` exists and nothing calls it, which is **O44**; the PATCH
routes are what make it reachable. `export::work` already writes
`<stem>.library.json` through `librarydoc::of`, so C3 §5's *"every edit writes
the file as well as the database"* reuses a path rather than adding one.

**`LibraryDoc` cannot express three of the fields C3 §5 lists as editable.** It
carries series titles, reading direction, tags and external ids; `volume`;
per-edition `language`, `kind` and `credits`; and per-member `chapter` and
`chapter_source`. It has **no work `title`** and **no edition `label`**. So PR 2
extends the document, and that is what puts **O67** in reach: a hand-set
`work.title` is the only thing that fixes a volume named
`Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz` while holding
four editions that string does not describe.

**Editing a title does not move the slug, and this is verified rather than
assumed.** O67 treats it as the blocker. Across `crates/taiyaku-library/src/`,
`slug` appears in exactly one `INSERT` — `import::insert_work`, fed by
`import::unique_slug` from the document's own title — and in **zero** `UPDATE`
statements anywhere in the crate. A slug is an address and a title is a display name, and the code
already agrees. PR 2's job is to pin that with a test, not to build it.

**Series membership is deferred, and the reason is a live collision rather than
a scoping preference.** `librarydoc::apply_series` writes
`UPDATE work SET series_id = ?1`, and `derive::write_work` writes
`UPDATE work SET series_id = ?1, volume = ?2`. So `apply` already sets series
membership and `derive` already overwrites it — for a scanned work, a hand edit
is a second input to derivation, exactly what C3 §3 refuses to let accept be,
and the next scan reverts it. The conflict is latent in shipped code today and
harmless only because O44 means nothing calls `apply`; **a PATCH route would
activate it.** Moving a scanned work between series is a rule operation, which
is what PR 1's accept path already is. An editor that works on imported works
and silently reverts on scanned ones is worse than no editor.

**How much of a real library that reverting half is has not been measured, and
the tempting figure is the wrong one.** C3 §1 cites *"about 90% of the library
single-edition"* from spec B §1; that is a statistic about editions per series,
not about scanned works as a share of all works, and using it here would be a
measurement whose noun had drifted. The argument above does not need a
proportion — one silently reverting edit is enough — so none is asserted.

`chapter_source` → `hand` comes free: `MemberMeta` carries it already, and C3
§5's argument — that a hand-set chapter is a different claim from a parsed one,
because `chapter_at` refuses to infer one from a filename — is why the column is
a closed set.

## 8. O62: a work whose files are gone, and a cost claim that was wrong

C1 §8's last empty state — *"a work whose members are missing from disk … shown
as unreadable rather than offered and then failing at the first page"* — was
deferred by C1 §5.5 to *"C3 with the rest of provenance"*, on this reasoning:

> It is not derivable *cheaply* — `member.path` is absolute and
> `scan_entry.rel_path` is relative to its root, so the join is over a computed
> string rather than an index. That is a question about what the store should
> record when a file vanishes […]

**That is wrong, and it is wrong in a way worth naming rather than quietly
routing around.** The computed string sits on the *probing* side of the join.
The side being probed is `scan_entry`, whose `UNIQUE (root_id, rel_path)` is
exactly the index the probe needs — and `member.root_id` has existed since
migration 6, which C1 itself shipped. A computed string defeats an index on
`member.path`; nothing wants one.

**Measured on this machine, 2026-08-26**, against a database built by applying
`schema/001`–`008` to an empty file:

```
QUERY PLAN
|--SCAN m
|--SEARCH r USING INTEGER PRIMARY KEY (rowid=?)
`--SEARCH se USING COVERING INDEX sqlite_autoindex_scan_entry_1
                                  (root_id=? AND rel_path=?) LEFT-JOIN
```

One pass over `member` — unavoidable, since the question is asked about every
member — and a covering-index probe per row. So O62 is **a defect with a known
fix, not a decision**, and no new column is needed. The store already records
enough; C1 §5.5's framing of it as *"what the store should record when a file
vanishes"* is answered *nothing further*.

Two alternatives were weighed and both lose to that. *Storing `member.rel_path`*
(migration 9, written by `derive::write_roots` where it already writes
`root_id`) removes the string arithmetic but duplicates a fact derivable from
two columns. *Marking `member.missing` in `forget_unseen`* reads cheapest but
turns a derived fact into a stored one that can go stale — a returning file
needs the flag cleared, and `root`'s `ON DELETE SET NULL` would leave the flag
asserting something false about a member whose root is gone. This project
stores a derived fact when re-deriving is lossy or expensive (`truth_member`'s
digests); here it is neither.

### The query, and the separator

```sql
LEFT JOIN scan_entry se
       ON se.root_id  = m.root_id
      AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
```

`derive` builds `member.path` as `root.path.join(rel_path)`, so the arithmetic
is exact rather than approximate. **`ltrim` rather than `length(r.path) + 2`**
because a root stored with a trailing separator makes the fixed offset drop the
first character of `rel_path` — verified on fixture rows for both root shapes,
`/srv/manga` and `/srv/other/`, which return the same `Series/vNN.cbz`.

### What it excludes, and what it reports

**A member with no `root_id` is not missing; it is not scanner-tracked.** That
is every imported, document-backed member — `import` writes no root — and also
a member whose root has since been removed, since `member.root_id` is
`ON DELETE SET NULL`. Absence of evidence is reported as absence of evidence,
which is the same refusal `quality_of` makes for a dangling alignment.

**Unreadable and incomplete are different statements and are not collapsed.**
An edition all of whose members are gone cannot be opened at all; an edition
missing some of them opens and then hits a hole partway through the page
sequence. C1 §8 names only the first, but the second is the same defect
arriving later, and the query answers both from one count. Reporting them as
one word would be the kind of drifted noun this spec has already corrected
twice.

The fact surfaces on the edition, since that is the level at which members
hang: `browse.rs`'s work detail and series-page rows carry it, and the read and
download controls are **absent** for an unreadable edition rather than disabled.
**`opds::work_feed`'s acquisition link goes too** — "offered and then fails" is
precisely the C2 case, where Panels on an iPad shows a download that 404s, and
suppressing it in the web UI while leaving it in the feed would fix the surface
that was never the complaint.

## 9. Testing

C3 §8's list, with the additions this design creates:

- **Accept writes a rule and re-derivation reproduces the same structure.** C3
  §8 calls this the important one; it fails if anyone makes accept a direct
  table write.
- **A widened rule accepts every proposal it resolves**, and `unaccept` on any
  one of them deletes the rule and reopens *all* of them — the failure §2 names.
- **`resolves` names collateral and `accept_with` refuses on it**, for a
  `regex` pattern as well as a `prefix` one. This is the `Publisher/` bug
  re-pinned on the path that did not exist when it was fixed.
- **A dismissed proposal survives a rescan that adds a volume to its subject.**
- The threshold hides weak proposals and the filter reveals them.
- **A metadata edit writes both the database and `<stem>.library.json`**,
  asserted on the file rather than on the call.
- **Editing a work's title leaves its slug unchanged**, with progress written
  before the edit still readable after it.
- Every evidence field renders when present and degrades when absent — a
  member of three pages must not render a broken interior panel.
- **A member whose `scan_entry` a rescan deleted reports its edition
  unreadable**, and the edition's read and download controls and its OPDS
  acquisition link are all absent rather than present-and-failing.
- **An edition missing some members reports incomplete, not unreadable**, and
  a member with no `root_id` reports neither.
- **The rel_path arithmetic holds for a root stored with a trailing
  separator**, which is the case a fixed `+ 2` offset gets wrong.

## 10. What this invalidates

- ~~`scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs`~~
  — **wrong, 2026-08-26.** `git diff main..HEAD -- scripts/` is empty: neither
  needed an edit, and neither was invalidated. Both read the numbers they check
  out of artifacts the run produces (`coverage-summary.json`, nextest's
  `junit.xml`) and compare them against prose in `docs/status.md`, so a branch
  that moves the numbers moves what they *report* and never what they *are*.
  The claim mistook "this branch will make these say something different" for
  "this branch will have to change these", which is the distinction those two
  scripts exist to draw. What did have to move is the prose in `status.md` —
  the next bullet, which was right.
- `docs/status.md` — the `web/` row, and both test counts
- `docs/sidecar.md` — `<stem>.library.json` gains a second writer
- **O44** and **O62** close in PR 2. **O53** and **O67** are re-read against
  what ships rather than promised closed here.
- **C1 §5.5's closing paragraph is wrong about the cost** and is marked in
  place rather than rewritten, per the open-items ritual: the reasoning is why
  the item sat where it did, and the next reader would otherwise re-derive it.
  O62's own row carries the same claim and needs the same treatment.

## 11. What could not be measured

- **Whether 37 proposals is a session or an afternoon**, unchanged from C3 §12.
  The inbox was chosen over a triage loop partly on a belief about a backlog
  nobody has worked through.
- ~~**What the `resolution` dry run costs on a real library.** It re-resolves
  every `scan_entry` through `rules::Engine` on each keystroke in the widening
  field. The corpus is small enough that this is very likely irrelevant and the
  form will be debounced regardless, but no timing has been taken and the
  sentence saying it is fine would be a guess.~~
  — **measured on this branch, 2026-08-26**, and recorded in `docs/notes.md`
  ("What the proposals inbox costs"): a table across 8, 5,008 and 25,008
  synthetic rows, and a re-measurement out to 250,005. Still synthetic rather
  than a real library, so the bullet's own noun is not fully discharged — but
  "no timing has been taken" is no longer true, and the form is debounced as
  predicted. The re-measurement also found the thing this bullet did not think
  to ask for: **the cost is per byte of `rel_path` swept rather than per row**,
  moving 2.4× at a fixed row count with path length alone, which is why two
  records of it on this branch disagreed two-fold until they were reconciled.
- **How long a ComicInfo stamp takes**, still, and now out of scope entirely.

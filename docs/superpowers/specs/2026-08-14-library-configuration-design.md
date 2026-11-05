# The library's configuration UI — design

**Status: settled in brainstorming on 2026-08-14. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **spec C4** of the roadmap in `2026-08-14-library-store-design.md`, §10 —
the second half of the C3/C4 split, which divided the remaining UI **by how
often it is used** rather than by subject. C3 is curation, done constantly from
inside the library. This is configuration: roots, scan control, the rule editor,
users and backup, touched rarely and from a settings area.

Depends on **C1**. The rule editor was designed against a live mockup, kept in
`.superpowers/brainstorm/` and gitignored.

Everything here is admin-gated per A2, and the CLI covers all of it in the
meantime — which it must anyway, since A2 §5 puts bootstrap there.

## 1. The rule editor, and what spec B's architecture actually bought

Spec B split facts from rules and claimed, in its §2, that **re-deriving the
whole library touches no filesystem**. That was argued as an architecture
property. This screen is what it purchases, and it is the one screen in this
product that Komga structurally cannot have: you edit a rule and see what it
does to all 156 series *before saving*.

**Layout.** An ordered list on the left — drag to reorder, since order is
semantic under B §4's per-field layering — with the editor and a live preview on
the right. Rules are enable/disable-able individually, and the whole list
imports and exports as JSON, per B §4.

**The preview has three tabs**, and each answers a different question:

- **Impact** — *"6 series merged, 71 works re-homed, 6 proposals resolved, 0
  works orphaned."* Computable before saving **only** because derivation is
  pure. A scanner that decided as it walked would have to re-read the disk to
  answer it.
- **Matched files** — a table of `directory → before → after`, and
  **deliberately including near-misses.** `Yu Yu Hakusho - Colored (Colored
  Council)` failing to match a colour-edition regex is worth more than five rows
  that succeed: a regex's *failure* is the thing you cannot see by reading it.
- **Provenance** — per field, which rule set it. This is B §4's stated
  requirement on this spec, and it is the price of layering: when the effective
  value comes from whichever rule set it last, an unexplainable result is the
  default outcome unless something shows the chain.

**Conflict detection is what makes layering safe to hand to people.** The mockup
found a real one in the seeded defaults: rule 4 sets `edition = raw` for
everything under `Manga (RAW)/`, and a later colour-edition rule sets
`edition = en-color` for anything ending "Full Color". Both match
`Manga (RAW)/Go-Toubun no Hanayome - Full Color`; the later wins; the Japanese
colour edition is labelled English. **That is correct behaviour producing a
wrong answer**, and it is invisible without the preview. Detecting that two
enabled rules set the same field on the same file is cheap, and the warning
names both rules and offers the two fixes — reorder, or narrow.

**Rules written by an accepted proposal are marked as such**, and deleting one
**reopens the proposal it came from** — the other end of C3 §3's
`proposal.created_rule_id`. Without that, the two screens would each hold half
of a reversible action and neither could complete it.

**Nothing is written until Save**, and the button states what it will do:
*"Save — apply to 71 works."*

## 2. Roots

Add, edit, remove. A root carries what spec A §3.5 gives it: `path`,
`edition_key`, `default_kind`, `default_language`, `exclude_globs`,
`scan_interval_secs`.

**Adding validates three things rather than one**, because the failures are
silent otherwise:

1. The path exists and is readable **by the server process**, which on a
   containerised deployment is not the same question as whether it exists.
2. It is not nested inside an existing root, and no existing root is nested
   inside it. Overlapping roots double-count every file underneath, and the
   symptom — duplicate works — looks nothing like the cause.
   *The overlapping-libraries design of 2026-11-02 has since narrowed this to
   the roots of one library: a root inside or around another library's root
   is accepted, save one at or inside a folder another root reads as one
   volume, and the outer root's walk skips it. See
   `2026-11-02-overlapping-libraries-design.md` §4.*
3. The watcher probe (§3) runs immediately, so the regime is known before the
   first scan rather than discovered later.

**Removing a root is the most destructive action in the product, and it does not
look like it.** Works derive from `scan_entry` + `rule`; dropping a root drops
its entries; re-derivation then produces fewer works — and **progress, notes and
read history hang off works.** So removal states counts before it acts: *"This
removes 412 works, 3 in-progress positions and 27 notes."*

An unmount is the same shape arriving by accident, which is why the scanner must
**never** treat "the root is empty this scan" as "delete everything" — a scan
finding zero entries under a previously-populated root is refused and reported,
not applied. That rule belongs to spec B's scanner and is recorded here because
this screen is where a person would otherwise meet its absence.

## 3. Scan control

Per root: last scan, its outcome, the watcher regime, the interval. Plus **scan
now**, incremental or full.

**The regime line is the output of an experiment, not a lookup** (spec B §8):

```
/srv/manga         watching 156 directories · scanning every 24h · last scan 2h ago, 0 changes
/mnt/remote/manga  watch probe failed · scanning every 15m · last scan 11m ago, 3 changes
```

**The scan report names what it did not do.** Following `inventory`'s discipline
— *"a candidate that reached the page threshold and then vanished from the
output is the silence this whole project is written against"* — a scan reports
entries found, containers it could not open, formats it recognised but cannot
read (`rar`, `7z`, `epub-reflow`), directories skipped by `exclude_globs`, and
proposals raised. A scan that quietly ignored 40 EPUBs and said "done" would be
the failure this project keeps writing rules against.

Live progress while a scan runs, and a scan already running is not startable
twice.

## 4. Users

List, create, edit. Per user: name, role, `can_download`, `can_stream`, the
roots they may see (A2 §8's `user_root`), their sessions and their API keys —
both revocable individually by an admin, because "a device was lost" is the case
that matters.

**Three refusals that stop this screen locking you out**, each of which is the
sort of thing that is obvious in hindsight and absent in practice:

1. **The last admin cannot be deleted.**
2. **An admin cannot remove their own admin role**, which is the same accident
   by a different route.
3. **A password reset does not silently revoke the current session** — A2 §4
   already says a change keeps the session that made it, and this screen must
   not have a second opinion.

**Deleting a user deletes their progress, notes and read history**, per spec A's
`ON DELETE CASCADE`. Stated with counts before it happens, for the same reason
as §2.

**No self-service registration.** An admin creates accounts, per A2 §11.

## 5. Backup

The store spec §13.4 designs it; this screen operates it. Schedule, retention
count, run now, list what exists, download one.

**Restore is deliberately out-of-band** — stop the server, move the file, start
it — and the screen says so rather than offering a button. A restore path
through a running server would have to quiesce every writer in the process, and
the value of that over "stop the server" is not worth the failure modes it
introduces.

**The screen states what a backup is not.** Sidecar export writes five documents
per work and carries none of the cross-work data — collections, tags, external
ids, users, read history — so a person who exported their library still needs
this. That distinction is invisible unless something says it.

## 6. The API

```
GET  /api/library/rules
POST /api/library/rules                 create
PATCH/DELETE /api/library/rules/{id}
POST /api/library/rules/preview         body: the full proposed rule list
POST /api/library/rules/import          body: the JSON export

GET/POST /api/library/roots
PATCH/DELETE /api/library/roots/{id}
POST /api/library/roots/{id}/scan       ?full=
GET  /api/library/roots/{id}/scan       status and the last report

GET/POST /api/library/users
PATCH/DELETE /api/library/users/{id}
DELETE /api/library/users/{id}/sessions/{sid}
DELETE /api/library/users/{id}/keys/{kid}

GET/POST /api/library/backups
```

**`rules/preview` takes the whole proposed list rather than one changed rule**,
and that is the load-bearing choice in this API. Layering means a rule's effect
depends on every rule around it, so a preview of one rule in isolation would
answer a question nobody asked. Sending the list also makes reorder-previews
free: dragging a rule is the same request.

It writes nothing and is safe to call on every keystroke, debounced.

## 7. Testing

- **`rules/preview` and a real save produce the same structure.** The whole
  screen is a promise that the preview is true; a test that pins them together
  is what keeps it one.
- **Conflict detection finds the seeded-defaults case from §1** — two rules
  setting `edition` on `Manga (RAW)/…- Full Color` — since that conflict exists
  in the shipped defaults and is the reason the feature exists.
- Reordering two rules changes the derived structure, which is per-field
  layering asserted rather than assumed.
- Deleting a proposal-written rule reopens its proposal.
- **Adding a nested root is refused**, both directions.
  *Since the overlapping-libraries design of 2026-11-02, only within one
  library (§4 of `2026-11-02-overlapping-libraries-design.md`).*
- **Removing a root reports work, progress and note counts before acting.**
- **The last admin cannot be deleted, and an admin cannot demote themselves.**
- A scan report lists unreadable formats rather than omitting them.

All of it lands in the `jsdom` project per C1 §9 — nothing here touches the
resolver.

## 8. What is deliberately not done

- **No in-browser restore.** §5.
- **No scan scheduling beyond an interval per root** — no cron expressions, no
  quiet hours. Nothing has produced the case.
- **No rule testing against arbitrary typed-in paths.** The preview runs against
  the real `scan_entry` table, which is more honest than a path someone invents
  and cheaper than building a second evaluator.
- **No user self-service anything.** Changing your own password and managing
  your own API keys is A2's account screen; this is the administrative view of
  other people.
- **No audit log.** A shared server would want one. Nothing here needs it yet,
  and a log nobody reads is a table that grows.

## 9. Branch shape

1. The rules API, including `preview` over a whole list.
2. The rule editor: list, reorder, edit, enable.
3. The three preview tabs, then conflict detection.
4. Roots: CRUD, the three validations, removal with counts.
5. Scan control, live progress, and the report.
6. Users, and the three refusals.
7. Backup.

## 10. The measurements this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs`
- `docs/status.md` — the `web/` row and the test counts
- `README.md` — configuring a library stops being a CLI-only activity

## 11. What could not be measured

- **Whether re-derivation is fast enough to preview on every keystroke.** The
  mockup shows "34 ms", which is **an illustration and not a measurement** —
  nothing has re-derived anything, because there is no store, no scanner and no
  library to re-derive. On 156 series it is likely fine and on a library of
  thousands it is unknown. If it is not, the fix is to debounce harder or
  preview on demand, and neither changes this design.
- **What a full scan of a real root costs**, which is what the interval defaults
  in §3 should be chosen from. Spec B carries the same gap.
- **Whether conflict detection is cheap at scale.** Comparing which rules set
  which fields on which files is quadratic in rules and linear in files; with
  seven rules that is nothing, and nobody has tried it with seventy.

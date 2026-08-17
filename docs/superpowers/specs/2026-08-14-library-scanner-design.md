# The library scanner — design

**Status: settled in brainstorming on 2026-08-14. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **spec B** of the roadmap in `2026-08-14-library-store-design.md`, §10.
It depends on A. It is what fills the library, and per §13.6 of that spec it
**stands alone**: importing a Komga or Stump library's structure is declined,
not deferred, so this is the only thing that ever produces one.

## Where it came from

Four constraints from the owner, on 2026-08-14, which between them rejected the
first version of this design:

> the server will end up running on this server, so it won't be over nfs

> in general I want others to be able to deploy this server, so our
> implementation of these things shouldn't necessarily be specific to my own
> setup

> the current directory structure is subject to change, and mostly exists
> because of the limitations of komga/stump that I'm seeking to solve

> what we should have is the ability to point to what files/directories contain
> what, and allow users to customize this […] these should have good defaults,
> but be overridable

The third is the one that matters most, and it is easy to miss: **the layout
this scanner was about to be designed against exists because Komga cannot group
editions.** Building to it would build to a workaround that this project
removes, and it would break the moment the workaround is no longer needed.

## 1. What was measured, and why most of it is not the design

A survey of the real library on 2026-08-14, read-only, over NFS from this
workstation:

- **156 series directories** across three roots — `Manga` 127,
  `Manga (RAW)` 10, `Manga (Chapters)` 19.
- Layout is uniform: `<root>/<series>/<series> - v01 (2023).cbz`, or
  `- c001.cbz` in the chapters root.
- **12 directory names appear in two or more roots**, and every one is
  genuinely the same series — including `Kaguya-sama wa Kokurasetai -
  Tensai-tachi no Ren'ai Zunosen` and `[Oshi no Ko] - Fan Translated`, both of
  which carry a ` - ` suffix. **Matching on the full name, suffix included, had
  no false positives.**
- **57 directories carry a ` - ` suffix; 37 of those have an unsuffixed prefix
  that also exists as a directory.** Of those 37, classified **by hand**:
  roughly **20 are true editions** (`- Full Color Edition`, `- Kanzenban`,
  `- Fan Translated`, `- Digital Colored Comics`) and roughly **17 are spin-offs
  or separate manga** (`- Kanna no Nichijo`, `- Kimetsu Gakuen!`,
  `- Lost Girls`, `- Kurapika's Memories`).
- **About 90% of the library is single-edition.** 127 series, 12 with a second
  edition anywhere.

**Two of those survive the reframe and the rest do not.** The layout facts
describe one library that is about to change. What survives is the two findings
about *judgement*, which are properties of naming rather than of this tree:

1. **Full-name matching is reliable** — 12 for 12, suffix included.
2. **Suffix-stripping as an edition signal is not** — roughly 20 right to 17
   wrong is barely better than a coin flip, which is why §6 proposes and never
   merges.

The 90% figure survives as corroboration of spec A §2: a Komga book is a work
with one edition, and that is not the exceptional case, it is nine cases in ten.

**One measurement was of the wrong host, and is recorded rather than deleted.**
The store spec's §10 warned that a `notify` watcher sees nothing over NFS, and
this survey confirmed the library sits on `192.168.1.69:/mnt/user/media 1` over
nfs4. That is this **workstation's** view. The server will run on the NAS
itself, where the same tree is local. §8 keeps the concern for other people's
deployments and replaces the mechanism that detects it.

## 2. Facts and rules

**A scan produces facts. Rules turn facts into structure.** The scanner records
what is on disk — paths, container formats, page counts, sizes, mtimes — and
decides nothing. An ordered, user-editable set of rules maps those facts onto
series, works, editions, volumes and chapters.

This is the whole architecture and it is what answers all four constraints at
once: nothing about `Manga (RAW)` is in the code, someone else's layout is a
different rule list, and a layout that changes is a rule edit.

**It also buys a property that a direct scanner cannot have.** The fact layer is
persisted, so **editing a rule re-derives the whole library without touching the
filesystem** — instant across 156 series, and free to experiment with. A scanner
that decided as it walked would have to re-read the disk to answer "what if the
rule were this instead".

## 3. The fact layer

```sql
scan_entry (id, root_id, rel_path, kind, format, size, mtime,
            pages NULL, sha256 NULL, first_seen, last_seen,
            UNIQUE(root_id, rel_path))
    -- kind:   'file' | 'dir'
    -- format: 'zip' | 'dir' | 'epub-fixed' | 'epub-reflow' | 'pdf' | 'rar' | '7z' | NULL

dir_state (root_id, rel_path, mtime, entry_count, last_walked,
           PK(root_id, rel_path)) WITHOUT ROWID
```

This is the derived scanner state spec A §3.5 deliberately deferred to here.

**Container format is sniffed by content, never by extension** — `taiyaku-read`
already does this, and the reason is recorded in `README.md`: extensions lie in
both directions, a zip is often named `.cbr` and a real RAR is often named
`.cbz`. **Formats nothing can read are recorded, not skipped**, per the store
spec §9: your `Light Novels` and `eBooks` roots guarantee the scanner meets
`epub-reflow` on its first run, and a scanner that silently drops what it cannot
open is the failure `inventory` was written against.

`sha256` is computed lazily — only when a member is about to become part of a
work — because hashing 156 series of archives on every scan is not affordable
and is not needed to know that a file exists and has not changed.

**Walking** reuses `inventory::walk`'s two-pass shape rather than its policy:
`totals` measures with no side effects, `decide` walks the same tree again, so a
volume split into chapter subdirectories is one candidate rather than seven —
`sidecar.md` records seven of fifty-seven corpus archives doing exactly that. Its
depth ceiling comes too, with the measured reason: `Path::is_dir()` swallows
`ELOOP` and answers `false`, so past ~40 levels a symlink cycle is silently
truncated rather than crashing, and the ceiling has to fire first.

## 4. The rule layer

```sql
rule (id, ordinal, enabled, root_id NULL, kind, pattern, match_on,
      set_edition_key NULL, set_kind NULL, set_language NULL,
      set_series NULL, note)
    -- kind:     'prefix' | 'regex' | 'builtin-volume' | 'builtin-chapter'
    -- match_on: 'path' | 'name' | 'parent'
```

**Ordered layering, per field.** Every matching rule applies, in `ordinal`
order, and a later rule overrides only the fields it actually sets. So
*"everything under this root is raw"* and *"except these are the colour
edition"* are two rules, not four — the alternative, first-match-wins, was
rejected because every exception would have to restate the general case, and
"most specific wins" was rejected because the engine would be making a
judgement the user cannot see or order.

Because the effective value of a field comes from whichever rule set it last,
**the UI must show provenance per field** — which rule decided this file is a
raw — or the layering becomes unexplainable. That is a requirement on spec C4,
recorded here because it is a consequence of this choice.

`root_id` NULL means the rule applies to every root.

**Four rule kinds, because regex is the escape hatch and not the interface:**

- **`prefix`** — a path prefix. *"This whole directory is raw."* One click in the
  UI, no pattern to write, and the case the owner named first.
- **`regex`** — Rust `regex`, with named captures `series`, `volume`, `chapter`,
  `edition`, `title`.
- **`builtin-volume`** / **`builtin-chapter`** — `inventory::volume_number` and
  its chapter sibling, called as a matcher.

**The builtins exist so there is one volume parser rather than two.**
`volume_number` is hand-written, measured on a 579-directory One Piece run, and
carries a guard a regex in a text box will not: a year *before* the marker, as
in `[2015] ONE PIECE v077`, must not be read as the volume. Re-expressing that
as a default regex would be a second implementation of a measured behaviour, and
it would drift.

**Defaults are seeded as ordinary rules**, visible and editable, with nothing
hidden in code:

| # | kind | match | sets |
| --- | --- | --- | --- |
| 1 | `regex` | `path`, first component under the root | `series` |
| 2 | `builtin-volume` | `name` | `volume` |
| 3 | `builtin-chapter` | `name` | `chapter` |
| 4 | `regex` | `name`, `^(?<series>.+?)[ _-]+v` | `series` |

Series defaults to the **parent directory** rather than a filename capture,
because a filename does not always repeat the series and a directory always
names it. Rule 4 layers a filename capture over it where one exists, which is
the layering model doing its job on its own defaults.

**User-supplied regex is safe here in a way it would not be elsewhere.** Rust's
`regex` crate has no backtracking and matches in time linear in the input, so a
pattern typed into a text box cannot produce catastrophic behaviour. This is
worth stating because "accept a user regex" is normally a ReDoS hazard and the
reason it is not one here is a property of the dependency rather than of the
design. It adds `regex` to `taiyaku-library`, which is a real dependency and is
named rather than slipped in.

**Rules export and import as JSON**, so a layout scheme can be shared or moved
between hosts. They live in the database — they are library state, versioned by
§13.4's backup — and not in a config file, which would be a second source of
truth that a UI writing rules would have to keep in step.

## 5. What a rule cannot do

**Cross-language aliases.** No pattern relates 「五等分の花嫁」 to
`Go-Toubun no Hanayome`. **`series_title` from migration 1 already solves it**:
a directory whose name matches any known title of a series joins that series.
The table exists for browse and search; this is a second use of it, and it means
consolidating two differently-named trees is a matter of adding a title rather
than writing a rule.

**Deciding whether a suffix means an edition.** §1's measurement says no rule
should try. §6 is what handles it.

## 6. Proposals

```sql
proposal (id, kind, subject_id, target_id, score, reason, state, created,
          created_rule_id NULL)
    -- kind:  'edition-of' | 'same-series' | 'attach-chapters' | 'align'
    -- state: 'open' | 'accepted' | 'dismissed'
```

**Dismissal has to persist**, or every rescan re-raises the same 37 suffixed
directories forever, and the person stops reading them — which is how a proposal
system becomes noise and then becomes ignored. `subject_id` therefore keys on
the **series row** rather than on anything content-derived: a dismissed
proposal whose subject is keyed on a content hash returns as a brand-new
proposal the moment a volume is added, and the dismissal silently stops meaning
anything.

### 6.1 Accepting writes a rule, and §2's claim depends on it

**Added 2026-08-14, while designing spec C3.** §2 claims that *editing a rule
re-derives the whole library without touching the filesystem*. That claim
survives only if derivation has exactly **one** input.

If accepting a proposal wrote directly to `series` and `edition`, structure
would come from two sources — rules and manual accepts — and re-deriving would
either discard the accepts or need a merge rule to protect them. That is
precisely the problem the store spec already had to solve once, with
`by: "hand"` and `Sidecar::merge`, and solving it a second time in a different
shape is how two mechanisms come to disagree.

**So accepting a proposal writes a `rule`.** A narrow one — a `prefix` rule
matching that exact directory — and "accept every case like this" is the same
mechanism with a broader `regex`. Derivation stays `scan_entry` + `rule` →
structure, with nothing else feeding it.

`proposal.created_rule_id` is the handle that makes this reversible: the
proposal row remembers the rule it wrote, so un-accepting deletes that rule and
reopens the proposal. Without it, an accept made before C4 ships the rule editor
would be a rule the person can neither see nor remove.

**Dismissal is not symmetric with acceptance and does not write a rule.** Doing
nothing to a proposal already has an outcome — the directory stays its own
series — so accepting *changes* that outcome while dismissing *confirms* it. A
dismissal feeds no derivation, so it stays a `state` change.

The pattern-level control for dismissal is a **score threshold**, not a
suppression list. Looked for and not found: a pattern covering the false
positives in §1's survey. They are `- Kanna no Nichijo`, `- Lost Girls`,
`- Gaiden`, `- Kurapika's Memories` — unrelated proper nouns whose only shared
property is scoring low, because the true cases carried a vocabulary match *and*
an equal volume count and these carry neither. One number, with the weak
proposals hidden but reachable, is what that shape actually wants.

Scored by an edition-word vocabulary — `Fan Translated`, `Color`/`Colour`,
`Kanzenban`, `Perfect Edition`, `Definitive Edition`, `Deluxe Edition`,
`Digital Colored Comics`, `Chromatic Edition`. **The vocabulary ranks; it never
decides.** §1's roughly-20-to-17 is the whole argument, and the vocabulary does
not rescue it: `Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)`
matches "Color Edition" and is the colour edition of a *spin-off*, so even the
best signal available gets that one wrong.

`reason` is stored as text and shown to the person, because a proposal whose
basis is invisible is a coin flip with a confidence score attached.

## 7. Works, editions, and the chapter case

A volume number in root R produces a **work** with one edition keyed by R's
`edition_key`, holding one member. The same volume of the same series in another
root joins that work as a second edition. Nine cases in ten stop there.

**A chapter-organised directory produces members and no work.** Which seven of
146 chapter files constitute volume 3 is a question only the alignment answers —
which is precisely the README's `taiyaku align raw-v01.cbz -- en-c001.cbz
en-c002.cbz …` case. So the members are recorded with `chapter_declared`, the
directory raises an `attach-chapters` proposal, and it stays visibly unattached
until someone acts. **Settled by the owner over the alternative**, which was
`sidecar.md`'s series-scoped work — measured and sound (three Go-Toubun volumes
in one pass, 578 of 585 paired) but it would turn 111-volume One Piece into one
work with 111 members and lose per-volume browse for any series that ever gains
a chapter edition.

**A defect in spec A that only a scanner surfaces.** That spec defines
`content_key` as *"a sha256 over the work's members' `sha256`s, taken in hub
edition order"*, which reads two ways, and one of them is broken: if it means
**all** members ordered hub-first, then **adding a second edition changes a
work's identity** — and adding editions is the point. It means the **hub
edition's members alone**, with the hub fixed when the work is created. Spec A
is corrected; this paragraph is the record of why.

## 8. Change detection

**Probe by experiment, not by filesystem type.** Whether inotify works is not
predictable from a fstype table: an Unraid user share is a FUSE overlay
(`shfs`), a Docker bind mount and `overlayfs` each behave differently, and NFS
delivers nothing for a change another host makes. A table of special cases would
be wrong for the next deployment. So on adding a root and at every startup, the
scanner **writes a temporary file into the root, watches for the event, and
records whether it arrived** within a timeout — one experiment that is correct
for every filesystem including ones nobody has thought of.

Three tiers, with the defaults chosen from that answer:

1. **Watcher** (`notify`, debounced). Primary where the probe succeeded.
2. **Interval.** Long where watching works — a safety net for events missed
   while the server was down. Short where it does not, because then it is the
   only mechanism.
3. **Cheap walk.** Compare `dir_state`'s mtime and entry count before
   descending; read a container only when `(size, mtime)` moved; hash only when
   §3 says a hash is needed. This is what makes either interval affordable.

**Reported per root, never assumed:**

```
root /srv/manga        watching 156 directories — scanning every 24h
root /mnt/remote/manga watch probe failed — scanning every 15m
```

A recursive watch costs one inotify watch per directory. This machine allows
524,288 watches and 1,024 instances (measured 2026-08-14) so it is not a
constraint here, but a lower ceiling elsewhere must be **reported** rather than
silently degrading to a partial watch — a partially-watched root is the worst of
the three states, because it looks like the first and behaves like neither.

### 8.1 An empty root is refused, never applied

**Added 2026-08-14, while designing spec C4.** A scan that finds **zero** entries
under a root that previously had them is **refused and reported**, not applied.

The reason is blast radius rather than likelihood. Works derive from
`scan_entry` + `rule`, and progress, notes and read history hang off works — so
applying an empty scan drops every entry, re-derives to no works, and takes a
person's reading positions and annotations with it. An unmounted NFS share, a
container started before its bind mount was ready, and a renamed directory all
produce exactly that scan, and none of the three is a rare event on a home
server.

The refusal is per root and appears in the scan summary, so the state is visible
rather than merely survived. A root that has genuinely become empty is emptied
by removing it, which spec C4 §2 does after stating its counts.

## 9. Auto-align: not now, and what would have to be true

`root.auto_align` is **removed from spec A's migration 1** rather than left
unused. Spec A's own rule is that migration 1 holds what A writes plus what is
expensive to backfill, and a config column nothing reads is neither.

Starting manual is the right order: a scan that silently spends hours
fingerprinting is the opposite of `inventory` and `duplicates`, which *print
what they found and leave the acting to a person*. The proposals of §6 cost
nothing until clicked.

**The note for later, so this is a decision rather than an omission.** Automatic
alignment becomes reasonable when three things are true, and none is today:

1. **The job runner exists** (project 2, specs D–F). Without it there is nothing
   to run an alignment *in*; the pipeline lives in `taiyaku-cli`, which depends
   on `taiyaku-serve` and so cannot be called from it.
2. **A pairing is not a guess.** The narrow case is safe and the broad one is
   not: volume 15 arriving in two roots for a series whose volumes 1–14 are
   already aligned is a pairing the library has evidence for. A first-ever
   pairing between two directories is exactly what §6's measurement says to
   propose rather than assume.
3. **The cost is known.** Nobody has measured what fingerprinting a new volume
   costs on the deployment host. Until someone has, "align it overnight" is a
   promise about a duration nobody has timed.

When those hold, it is a per-root setting defaulting **off**, restricted to case
2, and reported in the scan summary like everything else.

## 10. What is deliberately not done

- **No import of a Komga or Stump library**, per the store spec §13.6.
- **No automatic merging of series or editions.** §6.
- **No automatic alignment.** §9.
- **No EPUB or PDF reading.** They are *recorded* with a `format` and nothing
  opens them. Specs H and project 3.
- **No metadata scraping from AniList or MangaUpdates.** `external_id` is
  hand-matched, per the store spec §3.3.
- **No writing to the library.** The scanner reads. The one feature that writes
  is the store spec's §13.2 ComicInfo stamp, which is not this.
- **No rule editor UI.** Rules are seeded, and edited through the API or by
  importing JSON. The editor — including §4's per-field provenance display — is
  spec C4. Note that **C3 ships before it and writes rules** (§6.1), which is
  why `proposal.created_rule_id` exists: between C3 and C4, an accepted
  proposal's rule is reversible through the proposal that wrote it and is not
  otherwise visible.

## 11. Branch shape

One branch, one squashed PR:

1. `scan_entry`, `dir_state`, `rule`, `proposal` — migration 3.
2. The walk and the fact layer: container sniffing, page counts, no decisions.
3. The rule engine: four kinds, ordered layering, and the defaults as seeded
   rows.
4. Fact-to-structure derivation — series, works, editions, members — and the
   re-derive-without-walking path, which is §2's claim and needs a test.
5. Proposals and the scoring vocabulary.
6. Change detection: the probe, the watcher, the interval, the cheap walk.
7. `taiyaku library scan|rules` subcommands and the per-root report.

## 12. The measurements this invalidates

- `scripts/coverage-floor.mjs`, `scripts/status-test-count.mjs`
- `docs/status.md` — the test count, the dependency list, which gains `notify`
  and `regex`
- `README.md` — a `taiyaku library scan` synopsis, and the "Using it" section,
  where finding material stops being the person's job

## 13. What could not be measured

- **What a scan costs on the deployment host.** Every figure in §1 came from
  this workstation over NFS, which is the slow path and not the one that will
  run. The walk, the sniff and the first hash all want timing on the NAS before
  any interval default is defended.
- **Whether the watcher probe works on Unraid's `shfs`.** §8 chose an experiment
  precisely because this is unknown, but the experiment itself has not been run
  on that filesystem — it is designed to give a correct answer there, not proven
  to.
- **The 20-to-17 edition split is a hand classification**, made from knowing
  these series, not a measurement anyone else can reproduce from the data. It is
  strong enough to reject automatic merging and is not strong enough to be
  quoted as an accuracy figure.
- **Whether the default rules are good on anyone else's library.** They were
  written against one tree of 156 directories that is itself about to change.

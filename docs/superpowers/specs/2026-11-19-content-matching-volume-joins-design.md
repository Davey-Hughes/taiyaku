# Content matching PR 3: volume joins

2026-11-19. The third PR of `2026-11-15-edition-matching-by-content-design.md`
(the parent design), cut down to automatic joins. Align suggestions follow in a
PR of their own (Approaches).

## Why

The owner's goal (2026-11-15) is to move `Manga/Chapters` and `Manga/Raw` off
Komga onto the NAS taiyaku, with each series showing its English volumes and the
raw and fan editions beside them. Today editions join only where names agree:
`Raw/[Oshi no Ko]/Oshinoko v03` makes a work in a series `Oshinoko`, apart from
`Volumes/Oshi no Ko/Oshi no Ko - v03`. The owner ruled that high-confidence
content matches go automatic, but separable (parent design, rulings 1–2).
O340's test (#271) passed: no wrong volume pair of 12,186 reached V = 0.87,
and 52 of 59 true pairs did. This PR turns that result into joins.

## Goals

- A volume whose pages line up with another volume's at V ≥ 0.87 joins that
  volume's work as an edition, with no admin action.
- The page alignment of each joined pair is stored, so the ⇄ mark lights and
  the reader can switch editions at the matching page.
- Volumes a joined directory holds but that matched nothing (raw volumes past
  the last English one) follow the directory into the series it joined.
- An admin sees "matched by content" with the score on each joined edition and
  can Split it. A Split is remembered by every later scan, and can be undone.
- Nothing a reader saved (reading position, notes) is lost by a join or a Split.

## Non-goals

- Align suggestions for pairs below the bar. This PR stores their scores; the
  follow-up PR shows them on the Proposals screen.
- Chapters, duplicate copies and range packs (parent design PRs 4 and 5).
- Names as a penalty or a boost to the score (owner, 2026-11-16: names may only
  rank candidates or penalise across series; this PR uses them to rank only).
- A page index faster than brute force.
- Re-scoring settled pairs when the scorer or bar changes.

## Anti-goals

- A wrong join an admin cannot undo, or a Split that a later scan reverses.
- A join that moves or loses a reader's position or notes.
- A score that differs from the one O340 tested. The bar is only valid for the
  formula it was frozen against.
- Derive that gives a different library for the same files, rules and matches,
  or that writes on a re-run where nothing changed.
- A matching pass that blocks reading or scanning.

## Constraints

- **V is exactly O340's**: `features.rs` in the CLI's test-only
  `match_experiment`. The shorter side goes first, `taiyaku_core::solve` runs
  over all slots (spreads split into halves in reading order by
  `spread_flags`, across the whole edition), and V is the pages in paired runs
  of at least 4 (`LONG_RUN`) over the larger slot count. The bar is 0.87
  (`match_experiment/volume_test/bar.json`); a pair at exactly the bar joins.
- **Fingerprints are PR 2's** (`fingerprint` table, keyed by file digest). They
  store no low-information mark and no spread decision; both are computed at
  match time, as the test computed them.
- **Derive's invariants** (`derive.rs:3-26`): determinism, idempotence, no file
  opened, and the rule-editor preview runs inside a rolled-back transaction, so
  derive must read matches on the connection it is given. Reading a second input
  reverses the scanner spec's §6.1, as the parent design (§6) already rules.
- **Main deploys to the NAS on merge.** The PR is proven on a copy of real
  material before merge.
- The library's reading direction comes from `work.reading`; derive writes
  `rtl` for every work today.

## Approaches

The core has one sensible build: a table of matches beside the rules, read by
derive. Writing matches as rules cannot carry an alignment, a score or a Split
(the owner's ruling 6, 2026-11-16), and moving files after derive would be
undone by the next scan. The choice was how to cut it into PRs:

- **A. One PR with everything** (pass, table, joins, ⇄, mark, Split,
  suggestions). One merge, the largest diff.
- **B. Joins first, suggestions second.** This PR: the pass, the table, joins,
  ⇄, the mark and Split. A follow-up adds Align suggestions.
- **C. Look first.** A PR that only lists what would join, then one that joins.

**The owner chose B (2026-11-19).** Joins are what the goal needs, Split ships
with them so "automatic but separable" holds from the first merge, and each PR
is smaller than A. (I first gave "52 of 59 true pairs" as the joins' share of
the value. That overstated it: of the 52 pairs above the bar, 38 are colour
against black-and-white inside `Volumes` under one label, which this PR keeps
apart (§4), and the other 14 are raw against English, Tongari 11 and RuriDragon
3, some of which names already put in one work.)

## Rulings and calls this PR builds on

Owner (do not re-ask): match by content, high confidence automatic but separable
(2026-11-15); matches in their own table, a hand-written rule wins (2026-11-16);
names rank or penalise across series, never boost (2026-11-16); volume rule
passed at 0.87, PR 3 may join automatically (2026-11-19); approach B (2026-11-19).

Agreed on 2026-11-19: I proposed these and the owner answered "ok continue":
when joined series have different names the series takes the English volumes'
name; unmatched volumes follow their directory, as the parent design says; Align
suggestions from 0.6 up (stored here, shown in the follow-up); Split ships with
automatic joins.

## Design

### 1. Which files take part

A file takes part when all of these hold:

- it is the only member of its edition, in a work with a volume number;
- its fingerprint outcome is `whole`;
- at least one library with matching on reaches its directory.

A pair is matched only when one library with matching on reaches both files, so
Manga and Comics never join each other and the per-library switch keeps its
meaning. Range packs need no rule: nothing parses `v01-03` today, so a pack is
filed as volume 1, and its V against a single volume is about a third. Chapters
make no work, so they never take part.

### 2. The match pass

- **When.** After the fingerprint pass finishes (after a scan, at startup, when
  a library's switch is turned on), in the same chain. It has its own job kind
  and line in the scan feed ("Matching volumes · N of M") with Cancel, and
  resumes where it stopped: a file that has been looked up is recorded by
  digest and not looked up again.
- **Lookup.** Each file not yet looked up compares its informative slots
  (8–248 bits set) with every informative slot of every other file taking part,
  by brute force, and each slot votes for the file holding its nearest slot.
  The three files with the most votes are its candidates. This is PR 1's lookup
  (`match_experiment/lookup.rs`), which put the best partner in the top 3 for
  867 of 908 dev queries and 436 of 440 held-out ones, across every kind of
  file (`docs/notes.md`, around line 28504). It misses some, so a true pair the
  lookup never offers stays unjoined, which is the safe way to fail. Ties in votes go to the
  candidate with the same parsed series, then the same volume number, then the
  smaller digest; names never change a vote count.
- **Scoring.** Each candidate pair not already scored is scored with V, each
  side's slots built in its own work's reading direction. Pairs are stored by
  their two digests, so a rename or move keeps them, and a file whose bytes
  change is looked up afresh.
- **Stored.** Every pair at V ≥ 0.6, with V, the solver's runs and the figures
  the alignment row needs (verdict, slots, paired, average run, floor). Pairs
  under 0.6 store nothing but the looked-up mark.
- **After.** If the pass stored any pair at or above the bar, it runs a derive.
- **Cost.** PR 1 estimated the whole-NAS brute-force lookup at 0.48 h on all
  cores. A later pass looks up only new files. The slots of every file taking
  part are held in memory during a pass; the builder measures the release
  build's resident memory on the test library.

### 3. The match table

One row per scored pair (digest a < digest b): V, the runs, the alignment
figures, and a state: `scored` (from the pass) or `split` (an admin's
"not the same", with who and when). Whether a pair joins is not stored: derive
decides it from V, the state and the rules, each time. A separate record notes
each digest looked up, with the fingerprint and scorer versions it was looked up
under.

### 4. What derive does with matches

Derive groups by names as today, then applies matches.

- **Which matches apply.** A pair applies when V ≥ 0.87, its state is not
  `split`, both files take part (§1), the two editions have different keys, and
  the file that would move is not held by hand (below).
- **Same key, kept apart.** Two files under one edition key (a colour edition
  against the black-and-white one, both `en` from the `Volumes` directory) link
  nothing: they are copies under one label, which is PR 4's ground. An admin who
  wants the colour edition beside the black-and-white one gives its directory a
  key of its own with a rule, and the pair then joins.
- **Hand wins** (owner, 2026-11-19). The work that would move stays put when
  its series was set by a rule's literal `set_series` (derive records which rule
  set each field, `Resolved::provenance`), or when an admin set its title,
  volume or series by hand (`title_hand`, `volume_hand`, `series_id_hand`). A
  rule that reads names from a path, seeded or written by hand, never holds a
  file: content matching exists to overrule what names say. A rule that sets
  only an edition's key, kind or language holds nothing either, so the usual way
  to label a directory keeps it matchable. Hand edits on the host never stop a
  join.
- **Components.** Applied pairs link works into components. Each component
  becomes one work: the **host** is the work whose edition kind ranks highest
  (official, then fan, then raw), then the work in the older series, then the
  older work. The host keeps its id, series, volume, hub and reading
  direction; a joined edition is read in the host's direction.
- **Two possible hosts.** A file whose applied pairs lead to two works that
  cannot share a component (a raw matching both the black-and-white and the
  colour volume) joins the one it scores highest against, then the one with the
  same parsed series name, then the one in the older series.
- **Edition labels.** Each joining file keeps its own edition key, kind and
  language. A file whose key the component already holds stays out (that is a
  duplicate copy, PR 4's), and stays where its name put it. Files join in order
  of V against the host, highest first, so the outcome does not depend on
  scan order.
- **Directories follow.** A directory's joined files are tallied by the series
  their names put them in before the join. When most of one such series'
  joined files went to one target series, that series' works in the directory
  that joined nothing go to the target too, with their own volume numbers, but
  only for volume numbers the target lacks. The tally is per series, not per
  directory alone, because a flat or mixed directory holds several series, and
  one series' joins must not move another's volumes. Following is about a
  work's series, not about joining, so a work follows whatever its shape (a
  volume held in two copies, which cannot take part until PR 4, still
  follows). A work follows only when every one of its files sits in a directory
  that follows for its series. A volume number the target already holds means
  a match was expected and not found; that file stays where its name puts it,
  so a Split never has to undo a join it cannot see. A tie between series
  moves nothing.
- **Already together.** A pair that names already put in one work joins
  nothing, but its alignment is stored, so ⇄ lights for name-joined editions
  too. It shows no "matched by content" mark and no Split.
- **Alignments.** Derive writes an `alignment` row (`by = solver`) and its
  `run` rows for each applied pair, from the host's edition key to the guest's,
  replacing earlier solver rows for that pair and never touching `by = hand`
  rows. Stored runs are in whole pages. V is scored with spreads split into
  halves, but editions a scan makes carry no spread flags, and a run naming a
  half page is refused on them (`Sidecar::runs_of`, `sidecar.rs:1288`). Writing
  spread flags would change how readers see those pages and shift saved
  positions past them, so runs are cut where one side has a split spread and
  the other does not, and those stretches are left unpaired. ⇄ then lands on the
  nearest paired page there.
- **What moves with an edition.** When a joined file's edition moves into the
  host work, or back out on a Split, its reading positions, notes and anything
  else keyed to (work, edition key) move with it. `fold::absorb` cannot do this:
  it refuses works whose files differ.
- **An emptied series** (all its works joined elsewhere) is left as derive
  leaves one today: counted, not deleted.

### 5. Split and undo

- **Split** is an admin action on an edition joined by content. It sets the
  pair's state to `split` and runs a derive at once (a Rules save already does),
  so the file goes back to where its own name puts it. If that leaves its
  directory with no joins, its followers go back too.
- **Undo** sets the state back to `scored` and runs a derive; the pair joins
  again if it still applies.
- Both are admin-only routes, declared in the three route tables
  (`guarded_routes`, `grant_postures`, `narrowing_postures`).

### 6. What an admin sees

- On a volume's panel in the Series screen, each edition joined by content
  shows "matched by content · 0.93" and a Split button, with a confirmation.
- A volume kept apart by a Split shows "kept apart from <series> v05" with Undo,
  on both volumes' panels.
- The scan feed's new line, with Cancel.
- Readers see the joined editions and ⇄ as they would for name-joined ones; the
  mark and Split are admin-only.

The owner decides UI by trying it: these are prototyped on a live server with a
copy of the test library before tests are written for them.

### 7. Left for the follow-up PR

Align suggestions for stored pairs from 0.6 up to the bar, on the Proposals
screen, with accept and dismiss. The proposals table cannot hold them as it
stands (its identity is one series pair), so they will be read from the match
table. That PR adds no new scoring.

## Checks that prove it

1. **The scorer is O340's.** On the O340 material the server's scorer gives
   every one of the 12,246 pairs the V in
   `.superpowers/sdd/content-matching-o340-evidence/scores.tsv` to six decimals.
   This is a local run, kept as evidence; CI gets unit tests on fixtures.
2. **Real material.** A copy of `/home/davey/temp/manga-test` with the NAS's
   labels (Volumes `en`/official/en, Raw `ja`/raw/ja), plus a rule giving
   `Volumes/[Oshi no Ko] - Fan Translated` a key of its own, served and scanned.
   Before the run, a prediction is written for every series with both raw and
   English volumes: which files take part, which join, which follow, which stay
   apart and why. Expect many to stay apart for now: `Raw/[Oshi no Ko]` holds
   range packs (`v01-03`, `v04-05`, `v06-07`) and second copies (`v01b`, `v03s`,
   `v12b`, `v13A`), which today are filed into volumes 1, 3, 4, 6, 12 and 13 as
   extra members, so those volumes cannot take part until PR 4. After the run,
   every join, follower and non-join is checked against the prediction, and any
   difference is explained before merge. Also: ⇄ lights, a Split survives a
   rescan, Undo rejoins, and a reading position on a joined edition survives a
   join and a Split.
3. **The pass** shows its line, cancels, and resumes after a restart without
   looking up a file twice.
4. **Derive** run twice with no change writes nothing the second time, and the
   rule-editor preview shows joins without keeping them.
5. The project's usual gates, whole-branch review and mutation sweep.

## Left to the builder

- Where the scorer and lookup live so the server and the test both use them
  (the test's copy may stay frozen if moving it risks changing V).
- The table shapes and migration 029.
- How derive applies components inside its existing flow (`place` is the point
  where a file's own placement is already overridden for byte-identical copies).
- How the API carries the mark to the Series screen.

## My calls in this document

For the owner to check while reading:

1. **Same-key pairs stay apart** (§4): a colour edition and the black-and-white
   one under one label are not joined; a rule giving the colour directory its
   own key joins them. On O340's material that is 38 of the 52 pairs above
   the bar.
2. A pair is matched only within one library with matching on.
3. Only files that are the only member of their edition, in a work with a
   volume number and with a whole fingerprint, take part.
4. Three candidates per file, PR 1's lookup and vote.
5. The host is the official edition's work, then fan, then raw; then the older
   series. A file with two possible hosts joins the one it scores highest
   against.
6. A file held by hand (its series set by a rule's literal `set_series`, or its
   title, volume or series set by an admin) is not moved; rules that read names
   from paths, and rules that set only a key, kind or language, do not hold it;
   hand edits on the host never stop a join. (The owner ruled the literal
   criterion on 2026-11-19, over seed detection.)
7. Followers fill only volume numbers the series lacks, whatever their shape.
8. Name-joined pairs get their alignment (⇄) but no mark or Split.
9. Split and Undo run a derive at once.
10. Stored alignments are in whole pages; stretches where only one side has a
    split spread are left unpaired, and no spread flags are written.
11. Turning a library's matching off stops its joins at the next derive; the
    scores are kept, so turning it back on rejoins without re-scoring.

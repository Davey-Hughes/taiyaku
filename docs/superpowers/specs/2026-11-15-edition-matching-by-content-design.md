# Edition matching by content

Date: 2026-11-15. Status: approved by the owner 2026-11-16.

## Why

The owner keeps two kinds of manga in Komga that taiyaku cannot take yet:
`Manga/Chapters` (loose chapter files, fan translations and chapters newer than
the latest volume) and `Manga/Raw` (Japanese scans, many of them folders of
images with duplicate copies and range packs). The goal is to move both off
Komga onto the NAS taiyaku, with each series showing its English volumes and the
raw and fan editions beside them.

Today the scanner matches files by name and rules only
(`crates/taiyaku-library/src/derive.rs`, the seeded rules in
`schema/003_scanner.sql:129-133`). On the material under "Test material" that means:

- a raw folder named `Oshinoko v01` lands in a series `Oshinoko`, apart from the
  English `[Oshi no Ko]`;
- `Oshinoko v01`, `v01b`, `v01-03` and `v01-08b` all read as volume 1 under one
  edition key and are joined end to end into one long edition (`place()`,
  `derive.rs:1387-1482`);
- a chapter file with no volume makes no work at all; it raises an
  `attach-chapters` proposal that can never be accepted
  (`proposals.rs:57-66, 653-655`);
- `Megami Cafe no Terrace/` and `Megami no Cafe Terrace/` are two series,
  because series match only by exact `sort_title` (`derive.rs:1580-1596`).

The alignment tooling built at the start of the project already pairs pages by
content. Raw against English official paired 87.7–96.4% of pages over twelve
volumes in four series (`docs/notes.md:886-897`), and different series are
refused (`docs/notes.md:2463-2464`). None of it runs in the server: page
fingerprinting lives in `taiyaku-cli` (`fingerprint.rs`), a scan stores no
fingerprints and writes no `alignment` rows (`derive.rs:1692-1700`).

## Owner rulings (2026-11-15)

1. **Match by content.** "I would like the scanner to try to match image based
   files like manga/comics based on the tooling that was built at the beginning
   of the project. If we're able to derive a confidence score, then high
   confidence matches should be automatic (but separable), with suggestions only
   limited to low confidence."
2. **Content finds the candidates; names only break ties** (approach B under "Approaches").
3. **Chapters newer than any volume** become one "latest chapters" item after
   the last volume, e.g. "One Piece, chapters 1134–1176". It grows as chapters
   arrive; when a volume collects some of them, they move into it.
4. **Duplicate copies of one volume**: by default the best copy is shown and the
   others are kept as spares under that edition, switchable. An admin can turn
   on "each copy its own edition" per library instead.
5. **Omnibus editions stay whole** (2026-11-16). A Perfect Edition, Kanzenban
   or 3-in-1 book stays its own book in its own series. Only a file whose name
   carries a volume range (`v01-03`, `v01-07A`) is cut as a pack. Linking an
   omnibus to the ordinary volumes it covers is a follow-up.
6. **Matches live in their own table** (2026-11-16), read by derive beside the
   rules (Design §6).

## Goals

- A raw volume joins the English work it matches, as an edition, whatever its
  folder or file is called.
- Fan chapters that cover existing volumes become a fan edition of each volume
  they cover, holding the chapters that alignment places in it, in order.
- Chapters past the last volume form the "latest chapters" item (ruling 3).
- Two copies of one volume under one edition key stop being joined end to end
  (ruling 4). A range pack is cut into the volumes it holds.
- Every content match writes its page alignment, so switching editions in the
  reader keeps the page (today only imported sidecars carry one).
- A match above the confidence bar applies by itself; one below becomes a
  suggestion; any automatic match can be split off and stays split.

## Non-goals

- Grouping EPUB books in several translations (Harry Potter EN/KO, the classics
  under "Test material") into one work by scan. #249 reads and aligns books
  once they are one work, but that grouping still takes hand-written rules
  (O296). Text, not images; a separate piece of work after this one.
- Linking an omnibus edition to the ordinary volumes it covers (ruling 5): it
  needs alignment across works, which the store has no shape for
  (`alignment` rows sit inside one work, `001_initial.sql:101`). Follow-up.
- Credit-page detection. Credit pages stay unpaired gaps, as today; the paused
  frontier decision (`docs/notes.md`, "Fourteen fan editions…") is not reopened.
- Matching by name similarity (romanisation, hyphens). Content replaces it; the
  names only break ties between content candidates.

## Anti-goals

- **A wrong automatic match.** Pages from the wrong volume shown as this one is
  the failure that matters most; a missed match that waits as a suggestion is
  acceptable. The bar is set so that no wrong pair in the measurement clears it.
- **A scan the owner cannot use.** Fingerprinting must not hold back the works a
  scan finds, must show progress and must stop on Cancel (the first-scan work in
  flight, `docs/superpowers/specs/2026-11-15-first-scan-progress-and-cancel-design.md`).
- **A split that comes back.** Once an admin splits a match, no later scan
  re-applies it.

## Constraints

- Runs on the NAS host in a container with `/mnt/user` mounted read-only; the
  cost of fingerprinting there has never been measured (scanner spec §9,
  `2026-08-14-library-scanner-design.md:374-377`).
- Nothing is written inside a library directory.
- SQLite 3.40.1 where it runs (CI and the image).
- `taiyaku-cli` depends on `taiyaku-serve`, so the server cannot call the CLI's
  fingerprinting; it has to move into a crate both use. The solver
  (`taiyaku-core/src/solve.rs`) is already in the server's dependency graph.

## Approaches

- **A. Names pick the candidates, content confirms.** Pair files whose cleaned
  series names and volume numbers agree, then align them. Fingerprints only
  series with more than one edition, so it is cheap. Misses every case where the
  names differ (`Oshinoko` vs `[Oshi no Ko]`, Zelda's Japanese vs English title)
  unless someone fixes the names by hand.
- **B. Content finds the candidates, names break ties. CHOSEN by the owner.**
  Every page is fingerprinted once; a file's pages look up the nearest pages in
  the whole library and vote for the files they point to; the leaders are
  aligned. Finds pairs whatever they are called. Costs a fingerprint of every
  page in the library.
- **C. Align offline with the CLI and import sidecars.** No server cost, but the
  scanner would not match anything by itself; it fails ruling 1.

Exact-hash lookup and LSH do not work for the lookup in B: true matches differ
by about 40 of 256 bits (`docs/notes.md:1746-1756`). A brute-force Hamming scan
over stored fingerprints is the expected method, since the page-pair comparison
is cheap (`docs/notes.md:1756-1760`); PR 1 measures it at library size.

## Design

### 1. Fingerprints at scan time

- The CLI's fingerprint code (decode, per-member border crop, 16×16 dhash,
  spread halves) moves into a library-side crate; the CLI keeps using it.
- A fingerprint job runs after a scan's derive, so the works a scan finds appear
  first and are joined as matching completes. It reports into the first-scan
  work's shared progress feed and stops on its Cancel; stored fingerprints make
  an interrupted job resume where it stopped.
- Fingerprints are stored once per member content (keyed by digest), in the
  library database. A page with too little information to match (blank,
  near-uniform, a template divider) is marked as such and casts no vote:
  blanks score 1.000 against each other and dividers 0.97–0.99
  (`docs/notes.md:819-830`).

### 2. Candidates

For each member not yet matched, its pages look up their nearest stored pages
across the library and vote for the members those belong to. The members with
the strongest votes are the candidates. Names break ties: the same parsed series
or volume number ranks a candidate higher, never lower. Members whose files
changed are re-voted; settled matches are not re-run.

### 3. Confidence

Each candidate pair is aligned with the existing solver
(`taiyaku-core/src/solve.rs`). The confidence score and the automatic bar are
**set by PR 1's measurement, not here**. What is known going in:

- The per-pair margin cannot be the score on its own: the weakest good pair and
  an unrelated needle are one bit of 256 apart (`docs/notes.md:2481-2509`).
- Average run length separates needles (1.1–1.7 pages) from good volume pairs
  (36–200) (`docs/notes.md:2611-2675`).
- A single chapter against its own volume scored separation 0.254 and margin
  0.152, better than many good pairs, yet the verdict called it Fragmented
  because its runs average 6.8 (`docs/notes.md:2462`). The existing verdict is
  built for volume pairs; chapters need their own test (for example: the
  chapter's pages land in one contiguous stretch of the volume).
- Nobody has measured volume N against volume N±1 of one series.

If PR 1 finds no bar that keeps every wrong chapter pair out, chapter matches
start as suggestions only, and volume matches go automatic alone.

### 4. What a match does

- **Volume ↔ volume.** The matched member joins the other's work as an edition,
  under its own directory's edition key, and the page alignment is stored. The
  ⇄ mark on the series tile (`browse.rs:568-580`) then lights for it.
- **Chapters → volume.** The chapters that alignment places inside a volume
  become that volume's edition (under the chapters directory's key), in reading
  order, with the chapter-to-volume map stored. A chapter matching two volumes
  (a duplicated front page, `docs/notes.md:710-716`) goes where most of its
  pages land.
- **Chapters past the last volume** form the series' "latest chapters" item
  (ruling 3), placed after the last volume. When a later scan finds a volume
  that collects some of them, those chapters leave the item and join the volume.
  The item disappears when it is empty.
- **Duplicate copies** (two members matched as the same volume under one edition
  key). Default (ruling 4): the best copy is the edition's members; the others
  are spares of that edition, which a reader can switch to. "Best" is the copy
  with the larger pages and the fewest pages missing against the others; an
  admin can change the pick. Per-library setting: "each copy its own edition",
  where each copy gets its own edition under a derived key (`raw`, `raw-2`, …).
- **Range packs.** A member whose name carries a volume range (`v01-03`) and
  whose pages align with those volumes in turn is cut at the seams into page
  ranges, and each range is treated as a copy of that volume. This needs a
  member to be able to stand for part of a file. A member with one volume
  number that aligns with several volumes is an omnibus and stays whole
  (ruling 5): by content alone the two look the same, so the name decides.
- **A directory follows its matches** (my call). When a directory's members
  match into a series, its members that matched nothing (raw volumes past the
  last English one, chapters past them) go to that series too, instead of the
  series their own names give. Without this, `Raw/[Oshi no Ko]`'s files named
  `Oshinoko vNN` past the English v12 would stay in a series `Oshinoko`, and
  the fan chapters matching them with it. Where a directory's matches point at
  more than one series, the one most of its members match wins; a tie raises a
  suggestion instead.
- **When content has nothing** (my call). A directory none of whose members
  match anything (`Chapters/One Piece`, c1134–1176 only) takes its series from
  names, as today, with one addition: a chapter file's own name gives the
  series the way a volume file's does (`Megami no Cafe Terrace - c158` under
  the folder `Megami Cafe no Terrace/`). Names are all there is when content
  finds nothing, so this does not contradict ruling 2.
- **The latest-chapters item holds every chapter found in no volume you hold**
  (my reading of ruling 3), titled by its chapter range and placed after the
  last volume. Content is what says a chapter is in no volume: English volumes
  carry no chapter numbers, so "past the last volume" cannot be read from
  numbers. A chapter in the middle of a series whose volume you do not hold
  lands there too, under the same title rule.

### 5. Automatic, suggested, split

- A match at or above the bar applies by itself and is marked "matched by
  content" with its score wherever editions are listed for an admin.
- A match below the bar raises a proposal of the reserved `align` kind
  (`proposals.rs:53-54`) on the existing Proposals screen; accepting applies it,
  dismissing stops it being raised again for that pair.
- **Split** (an admin action on any content match, automatic or accepted)
  undoes it and records "not the same" for that pair, which every later scan
  respects. An admin can undo a split.

### 6. Where matches live (ruling 6)

Accepting a proposal today writes a rule, and derivation reads facts plus rules
only (scanner spec §6.1, `2026-08-14-library-scanner-design.md:229-268`). Rules
are path patterns: they cannot carry a page range, a chapter-to-volume map, a
score or a "not the same" record. So content matches are stored as their own
table, which derive reads as a second input beside the rules. This reverses
§6.1's "exactly one input". A rule written by hand still wins over a content
match for the same file.

### 7. Which libraries, and the cost (my calls)

- Matching is a per-library switch, on by default, so an admin can keep it off
  a library whose material was never measured or whose first pass costs too
  much.
- PR 1 measures one comics pair (an issues folder against its collected
  edition, copied from the NAS's `Comics/`) as well as manga, and treats
  `One Piece - Cover Stories - Digital Colored Comics` against the One Piece
  volumes as a wrong-pair class: cover-story pages reappear on chapter title
  pages, a likely near miss.
- The fingerprint job runs in the background at low priority, the library is
  usable throughout, and it resumes after Cancel or a restart. If PR 1 measures
  a first pass over the NAS library at more than a day, this comes back to the
  owner before PR 2 is planned.

## Rulings this reverses

- Scanner spec §7 (`2026-08-14-library-scanner-design.md:276-299`): "a
  chapter-organised directory produces no work". Its reason was that only
  alignment can say which chapters make a volume; alignment now says it. The
  `attach-chapters` refusal (`proposals.rs:57-66`) goes with it.
- Scanner spec §10 (`:381-398`): "no automatic merging of series or editions".
  Now automatic above the bar, and separable (ruling 1).
- Scanner spec §6.1: one input to derivation (Design §6 here).

## Delivery

Several PRs, each through the usual plan and review:

1. **Measurement, no product change.** On the material under "Test material": score every true
   pair (raw↔EN volume, chapter↔its volume, copy↔copy, pack↔its volumes) and
   wrong ones (volume N↔N±1, chapter↔the neighbouring volume, other series),
   and the library-wide lookup's vote margins. Write the decision rule down
   before running it (as the credit survey did,
   `docs/notes.md`, "Fourteen fan editions…"). Time fingerprinting on the NAS in
   a release build. Result: the score, the bar, and whether chapters can go
   automatic.
2. **Fingerprints in the server** (Design §1): the job, storage, progress,
   Cancel. Depends on the first-scan work merging (its progress feed and
   Cancel are on the unmerged `first-scan` branch, and its spec says the feed
   covers scans only), then extends that feed to the fingerprint job.
3. **Volume matching** (Design §2–§6, for volume ↔ volume): candidates, alignment,
   automatic and suggested matches, split, stored alignments.
4. **Duplicates and range packs** (§4), with the per-library setting.
5. **Chapters** (§4): chapter-to-volume editions and the latest-chapters item.

## Left to the builder

- The confidence formula and bar (from PR 1).
- How many candidates per member to align, and the vote threshold.
- The storage shape of fingerprints, matches and splits, and the migration.
- How a part-of-file member is represented.
- Where the "matched by content" mark and Split appear in the web UI, within the
  existing edition lists and Proposals screen.
- The order of PRs 4 and 5.

## Test material

Copied 2026-11-15 from the NAS into `/home/davey/temp/manga-test`, NAS layout
kept: `Raw/` (9 series, including Oshi no Ko, Kaguya and Kobayashi with
duplicates and range packs), `Chapters/` (10 series), `Books/Literature/`
(8 authors, for the later books work), and 5 English series added to `Volumes/`
as partners for name-mismatch cases. The new folders are not yet roots in the
local `library.db`. No comics are in the test library yet; PR 1 copies one
Comics issues + collected pair from the NAS (Design §7).

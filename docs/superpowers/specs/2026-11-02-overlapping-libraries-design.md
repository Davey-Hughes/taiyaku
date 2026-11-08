# Libraries whose directories overlap

Asked for by the user on 2026-10-18, while trying O219's Libraries pages:
"can we add a task later to examine whether we can instead have libraries that
overlap folders?" `docs/status.md` carried it under Later, with a survey of the
assumptions as the first step. The survey was read at `main` 6067071d, in two
halves (the store and scanner; serve, the web, the CLI and the docs), with the
store half's questions also run against scratch stores. This document is what
the user decided from it, and the three PRs that build it.

Cases used throughout: **N**, nested across libraries (Manga holds `/m`, Kids
holds `/m/kids`); **S**, the same directory in two libraries; **W**, nested
within one library.

## Decisions the user made, 2026-11-02

1. **What an overlap is for.** A book can be in more than one library, for
   two reasons: different slices per account (a wide library for some
   accounts, a narrow one for others), and several groupings of the same books
   for browsing. "Adding a parent absorbs the nested directory, so a book
   stays in one library" was offered and not chosen.
2. **The inner directory decides.** A file is scanned once, under the deepest
   directory any library holds, and that directory's rules and settings name
   it. An outer library shows it too: once, with one reading position.
   Chosen over: only the outermost directory scanned, with inner libraries as
   views; a copy per library; each library scanning its own directories with
   shared files merged.
3. **S is in this work.** Two libraries may hold the very same directory.
   Recommended against ("not now"); the user chose it.
4. **Series and editions are joined across all libraries.** Internally there is
   one derive over the whole store; an account is shown only the editions it
   has access to. In the user's words: "they should be joined across all
   libraries internally, however only editions that an account has access too
   should be shown to that account." Recommended instead: joining only across
   libraries that share or nest a directory.
5. **The model (§1) and the three PRs (§3–§5)** were approved as presented. The
   series page carrying `?library=` (§3.6) was part of the PR list approved.

Decision 4 reverses three of the libraries design's decisions of 2026-10-05
(`2026-10-05-libraries-design.md`, decisions 1–3); §6 lists everything it
reverses or reopens.

## Decisions the user made, 2026-11-03

PR 1's drive on real material confirmed O239 (two different files of one
volume, under one edition key in two libraries' directories, join one
edition) and found that §1 as first written served the whole edition to an
account granted one of those libraries.

6. **A merged edition is narrowed, and administrators are shown each case.**
   In the user's words: "I would rather narrow the pages but we should surface
   these cases to the administrator under a new admin page so it's clear."
   Chosen over serving the whole edition as §1 said (recommended) and over
   failing closed. It is part of PR 1; recommended instead: its own PR, with
   PR 1 waiting.
7. **A narrowed edition is renumbered.** The account sees pages 1 to n; stored
   positions and notes stay in the full numbering and are translated where
   they cross the server's edge.
8. **A narrowed account reads, keeps a position and writes notes.** Alignments
   that touch a narrowed edition are left out of what it is sent; writing
   ground truth or correcting an alignment on a narrowed edition is refused; a
   spread is kept only where both its halves are visible.
9. **The cases are listed on a new Configuration tab**, for administrators
   only, computed when the page is opened.
10. **The design in §3.8** was approved as presented.
11. **The notes and truth `ETag` names the account's numbering**, ruled on
    PR 1's Task 15 review: under the stored document's `ETag` alone, a write
    read before a grant change or a scan passed `If-Match` and was applied in
    the new numbering, silently deleting and moving notes. Chosen over also
    making the web reader reload on such a 412, and over keeping the stored
    document's `ETag` and recording the risk. What remains, an open item: on a
    412 the web re-applies its own unsaved notes in the numbering it read.

## Decisions the user made, 2026-11-04

PR 1's whole-branch review found that a byte-identical file under two
directories whose rules file it differently (another series or volume, or
another path for a file with no volume) made two groups of one content key,
which wrote one work row in turn and left one directory's library without it;
and that a series page opened from a library counted a merged edition by that
library's share of it.

12. **A byte-identical file is one work wherever it is held.** Copies collapse
    across the store by `sha256`: one work, with a `member_root` row for every
    directory holding the file, named by the first directory's rules, seen
    through either library's grant, and kept when one of the directories is
    removed. Chosen over adding the other directory as a copy while the work
    keeps whichever name the sort order gives it, and over accepting the case
    with an open item.
13. **A partial copy collapses per file.** Each byte-identical file becomes a
    copy in the first directory's work, and the copy directory's other files
    stay in the work their own rules name. A volume that shares some of its
    files with another directory's volume therefore splits: an account granted
    only the copy directory's library sees the first work narrowed to the
    shared files (decision 6), and a second work holding the rest. Chosen over
    the whole group joining the first directory's work, and over collapsing
    only a group copied whole.
14. **The first directory is the held one, else the oldest.** Of the
    directories holding a file, the one its held member already names stays
    the primary; with none held, the directory with the lowest id.
15. **A series page counts what the account opens.** An edition's page count
    on a series page, and the count in its download link's label, are taken
    by the account's own grants, whichever library the page was opened from:
    an administrator opening it from Kids sees a merged edition whole, and an
    account granted only Kids sees Kids' share. `?library=` still chooses
    which works are listed. Chosen over counting the library's share and
    marking the page as narrowed.

An observation, not a ruling: as decision 14 keeps the primary file by file,
the works a store holds depend on the order its files arrived in. A volume
whose files became copies one at a time stays the works they were first held
in: with `c001` held under A and `c002` under B, each directory then receiving
the other's file leaves two works of one file, each held under both, where a
store derived from the same files at once makes one work of two. It follows
from decisions 13 and 14 as worded, and no ruling covers it.

PR 2 added eight: the first while its plan was written, the next four on its
directory and library pages, and the last three in its whole-branch review.

16. **"Scan library" walks every directory the library shows**: its own
    directories and each nested directory it reaches, each scanned as its own
    directory by its own rules. The CLI's `scan --library` does the same.
    Chosen over its own directories only.
17. **A directory's page says what nests in it as a count and a fold.** An
    outer directory draws one muted disclosure, "N folders inside use their
    own library's rules", which opens to a row per nested directory: its path
    relative to this one, its library, and "also shown in <this library>" or
    "kept out by this directory's exclusions". An inner directory draws one
    muted line per directory around it, "Also shown in <library> (inside
    <path>)". Chosen over a line beside the rules, and over the Libraries
    list alone.
18. **A directory's page greys out only what the server refuses while a scan
    runs around or inside it.** The user chose this on the premise that the
    server then refuses only Remove…, and that premise was wrong: a move is
    refused whenever the server's job lock is held, and a running scan of any
    library holds it, as a rule save, a stamp or another move does. So the
    page holds Remove… while a scan around or inside the directory holds its
    removal (`held_removals` in the library's scan status), and Move… while
    the job lock is held (`moves_held`). It therefore holds Move… during such
    a scan, which the ruling, made on the wrong premise, expected to stay free.
19. **The kept-out text says which rule keeps a nested directory out.** An
    exclusion pattern keeps decision 17's wording. The rule that skips names
    starting with a dot reads "kept out: names starting with a dot are
    skipped" on the outer directory's page and "Kept out of <library>: its
    scans skip names starting with a dot (inside <path>)" on the inner's; the
    server says which rule it was.
20. **A library's card says when a nested library scans what it shows**:
    "Kids is scanning kids/, which Manga shows."
21. **The hand-over moves only the members of files the nested directory's
    scan recorded.** A file its own patterns exclude stays with the outer
    directory: the outer library's accounts keep seeing it, the nested
    library's never do, and removing the nested directory loses nothing of
    it. A file the nested directory comes to exclude after its scan took the
    file over goes back to the outer directory when the nested one is
    removed, where the outer's patterns admit it. While the nested directory
    exists the file is not refreshed, as a file an in-root pattern excludes
    is not (O92). Chosen over the plan's
    every member moving, which showed the nested library's accounts what its
    patterns kept out, and deleted that work with its positions when the
    nested directory was removed.
22. **A directory's page greys out only what the server refuses, a sibling's
    scan included.** Decision 18's rule is general. While another directory
    of the same library has a scan reserved or running, the page holds
    Move… alone: Scan directory queues behind the other scan, and the server
    takes Remove…. A scan of the whole library reserves this directory too,
    so the page holds all three then. A removal that lands while a scan
    derives can leave members with no directory until a later scan (O249).
    Chosen over holding Remove… during every scan.
23. **New library's Directory path help** reads "The server checks that it
    can read the directory. It may sit inside or around another library's
    directory." Chosen over naming the overlaps a new library is refused.
    Decision 26 replaces this wording.

## Decisions the user made, 2026-11-05

24. **A shared directory's page has two Removal rows.** Only while another
    library holds the directory does Removal list "Remove from Manga" ("Kids
    keeps it; nothing is deleted.") and "Remove this directory" ("From every
    library that holds it, with what it would take."). A directory in one
    library keeps its one Remove…. Remove from this library never deletes:
    on a directory's last library it is refused, and the refusal points to
    Remove this directory. Chosen over an unlink that removes the directory
    when it is the last link.
25. **Move to a library that already holds the directory is refused**
    ("`/m` is already in Kids"), and such a library is left out of Move's
    list. Chosen over treating it as an unlink.
26. **New library's Directory path help** reads "The server checks that it
    can read the directory. It may be another library's directory too, or
    sit inside or around one."
27. **A scan of a shared directory names the library whose page started
    it.** The page sends its library with the scan, and the other library's
    card reads "Manga is scanning `/m`, which Kids holds too." A scan asked
    for without a library names none. Decision 20's line for a nested
    directory stands. Chosen over filing every directory scan under no
    library.

## Decisions the user made, 2026-11-07

28. **A library's Directories table says which directories overlap.** Its
    rows gain a small column, Overlaps, that says at a glance whether a
    directory overlaps another library's directory, and how, without naming
    the library: "Shared" where another library holds the same directory,
    "Inside" where it sits inside another library's directory, "Around"
    where another library's directory sits inside it, several joined where
    more than one holds ("Shared, Inside, Around"), and blank otherwise. At
    a phone's width the row reads "Overlaps: Shared", as its other values
    follow their labels. Chosen over marking shared directories only, and
    over a bare mark.
29. **Split editions link a directory several libraries hold once**, under
    the first library the edition lists. Chosen over a link under each
    library, which would give one path several links that could not be told
    apart. Neither reveals anything to an account: the tab and every
    directory page it links to are an administrator's alone, refused to
    anyone else before they are read.
30. **A directory scan for a library that does not hold the directory
    answers 404**, "`/m` is not in Kids" or "there is no library N", as
    Remove from this library and Move answer the same refusals. A
    `?library=` that is not a number, like any query the scan cannot read,
    is refused 400 in the `{"error": …}` shape, as the unlink and Move
    refuse a query they cannot read. Chosen over the 400 the plan gave
    both.

## 1. The model

- **A library is a name and a set of directories.** It decides who sees what
  (grants) and what a library page lists (`?library=`). It no longer decides
  what a work or a series is.
- **One derive over the whole store.** Series and editions join wherever their
  files are, as they did before F1. A byte-identical volume in two libraries is
  one work, with one position per account. A byte-identical file is one member
  wherever it is held, named by the rules of its first directory: the one its
  held member names, else the oldest (decisions 12–14).
- **Each file is scanned once**, under the deepest directory that holds it, and
  that directory's rules and per-directory settings (`edition_key`,
  `default_kind`, `default_language`, `exclude_globs`, `keeps_documents`) name
  it.
- **What an account sees.** An edition is visible if any library the account
  is granted reaches a directory holding one of the edition's members. A work
  or series is visible if any of its editions is, and then shows only those
  editions. A library reaches its own directories and every directory nested
  inside them, except a nested directory that the outer directory's
  `exclude_globs` leave out. An account with "every library" (O220) sees
  everything, as now. A visible edition shows the account only its members
  under such directories: an edition merged from different files in two
  libraries' directories is narrowed for an account that can see only some of
  them (§3.8).
- **Moving a directory** to another library changes who sees it and nothing
  else: works, positions and notes stay where they are.
- **Removing a directory** deletes only what no remaining directory holds.
- **Still refused:** a directory equal to, inside or around another directory
  of the *same* library, by add and by Move. That is what keeps O47's doubled
  pages fixed.

## 2. What the survey found in the way

Each of these is one place where today's store or server assumes a file is
reached through one directory in one library:

- `scan::vetted` (`taiyaku-library/src/scan.rs`) refuses N, S and W against
  every directory of every library; and `root.path` is `UNIQUE`
  (`001_initial.sql`), so S is refused by the schema as well.
- `root.library_id` is one column (`015_libraries.sql`), read by the derive,
  `libraries::list`, `libraries::books` and about eleven sites in serve, the
  CLI and the web.
- The derive's group key includes the library (`derive.rs`, `Group` and its
  key), `work` is unique on `(content_key, library_id)`, and series are filed by
  `series.library_id` (`015_libraries.sql`), so a work never spans libraries.
  Simply deleting `vetted`'s check therefore gives one *copy* of each shared book
  per library (run): an account granted both sees it twice, with two positions.
- `member.root_id` is one column (`006_member_root.sql`). When one hash is held
  by two roots, `import::held_member_meta` sets it NULL, which fails *closed*:
  the member is then outside every grant (`HeldMember::root_id`'s doc). Under
  decision 4 a copy in two directories is the normal case, so one column cannot
  carry access.
- Rules match the root-relative path, and the seeded rules take the series from
  the first directory under the root, so the same file can be series `x`
  through `/m` and `Bleach` through `/m/x` (run). Decision 2 answers whose view
  decides.
- Proposals pair only series of one library (`proposals::raise`, decision 3 of
  2026-10-05).
- Move (`relocate.rs`) has no nesting check of its own; moving a directory into
  a library holding one around it recreates O47 (run).
- Removal counts the works a root alone accounts for (`scan.rs`,
  `DOOMED_WORKS`, `take_root`).
- **Access already works per directory.** Grants and `?library=` test whether
  an edition has a member under a granted directory, at five SQL sites in
  `browse.rs` and `metadata.rs` and through two resolvers
  (`account::access_for`, `browse::library_scope`). Neither a work's nor a
  series' library is read there. The only serve readers of `work.library_id`
  are OPDS's by-work library feed (`browse::filed_in`) and the Libraries works
  count (`libraries::list`).

Found on the way, and not part of this work: a symlinked directory inside one
root doubles a book's pages on `main` today (run), the O47 shape with no
nested root at all. It is filed as its own row.

## 3. PR 1: one derive across libraries

Reverses decisions 1–3 of 2026-10-05. After it, libraries still cannot overlap
(`vetted` is unchanged), but a work can have editions in several libraries.

### 3.1 The schema

A new migration (`018`):

- `work` and `series` lose `library_id`; `work.content_key` is unique across
  the store again.
- A `member_root (member_id, root_id)` table records every directory that holds
  a copy of a member, one row per copy. It replaces `member.root_id` as the
  thing access reads. `member.root_id` stays, as the directory of the copy
  `member.path` names, which the hand document's address, `superseded`'s key
  and the scan-entry joins read; two triggers keep that directory in
  `member_root`.
- **Copies are reduced to the oldest row, not folded**, by the user's ruling of
  2026-11-02 that no store holds data worth carrying. Of each `content_key` the
  lowest-id work is kept, and of each `sort_title` the lowest-id series, with
  every work re-pointed to the kept series; the rest is deleted with every row
  that names it, positions and notes included. The kept row's slug and id
  survive, and links to a deleted row's slug stop resolving. The migration
  runs on open like any other, takes no backup of its own and reports nothing,
  and the next scan writes the kept rows again from the files.

### 3.2 The derive and identity

- The group key loses the library; `reconcile_series` reconciles across the
  store.
- `import::find_by_key` and `derive::held_work` lose their library scope, and
  `libraries::own_or_adoptable_*` go.
- `held_member_meta`'s NULL-on-two-roots rule goes: two roots holding one hash
  becomes two `member_root` rows.
- **Copies collapse by `sha256`, across the store** (decisions 12–14).
  `derive::classify` stages each file with its own directory's series, volume
  and edition key, and `derive::place` makes a file held under several
  directories one member, filed where its primary directory's rules put it,
  with a `member_root` row for each of the others. `derive::keep_primaries`
  chooses the primary: the directory a held member of those bytes names, else
  the lowest directory id. A volume sharing some of its files with another
  directory's volume splits per file; two files of one hash under one directory
  stay two members.

### 3.3 Proposals

Pairing runs across the store (`raise_edition_of`, `raise_same_series`,
`raise_attach_chapters`). A proposal's rule is still scoped to one directory or
to every directory (decision 4 of 2026-10-05 stands); `common_directory_pattern`
still refuses a subject spread over two directories.

### 3.4 Access and counts

- The five SQL sites test `member_root` instead of `member.root_id`.
- `libraries::list`'s works count and OPDS's by-work library feed count the
  works with a visible member under the library's directories.

### 3.5 Move and removal

- **Move** becomes relabelling the directory's library. The split, join and
  fold steps of the libraries design §6.2 go, with their counts; Move's dry run
  says who will gain and lose sight of the directory instead.
- **Removal** deletes the `member_root` rows of the removed directory, then
  every member left with none, then every work left with no member, as now.
  The dry run counts the same. O220's no-library rule is unchanged.

### 3.6 The series page carries `?library=`

A series can now hold works from several libraries, so a series card in Kids
can count one volume while the page it opens, for an account granted both,
shows five. The series page takes the library it was opened from:
`browse::one_series` and its route take `library`, the grid's link carries it,
and the reader-side caches key on it. The library chooses which works and
editions the page lists; each edition's files, pages and missing files are
counted by the account's own grants (decision 15), so a cell and its download
label promise what the reader and the download serve.

### 3.7 Docs

`docs/status.md`'s libraries summary ("Derivation and proposals stay inside one
library…") is rewritten; the 2026-10-05 spec's decisions 1–3, §2.2 and §3's "a
series is in exactly one library" get a note pointing here; O191's row is
re-read (§6).

### 3.8 Narrowing a merged edition (decisions 6–11)

**When an edition is narrowed.** A member is visible to an account when one of
its `member_root` rows names a directory that a library granted to the account
reaches: the test `browse::member_clause` already makes. An edition with every
member visible is served as now, one with none is hidden as now, and one with
some is narrowed. Administrators and "every library" accounts are never
narrowed. The derive, migration 018, the tables and the web reader do not
change: the reader is sent a shorter edition and reads it like any other.

**The view.** Core gains one function: given an edition and which of its
members are visible, it returns the narrowed edition and the page map between
the two numberings, both ways. The narrowed edition holds the visible members
in their order; its pages are theirs, renumbered from 0. Spreads and credit
pages are page indices, kept where their page is visible and renumbered: a
spread is one wide page, so it is never half hidden. Slots are counted over the
narrowed pages, and a position's or a note's slot is translated through its
page and half. Serve gains one function that gives an
account's view of a work, built per request from the one shared in-memory
sidecar, and every route below reads it.

**What each route does with a narrowed edition.**

- **The work document** (`GET /api/works/{work}`) carries the narrowed
  edition, and leaves out every alignment that touches a narrowed edition. Its
  `ETag` is taken over the document sent.
- **Pages and covers.** A page, its thumb, the page `ETag` lookup, the OPDS
  page stream, the OPDS cover and OPDS's `pse:count` translate the narrowed
  index into the full one before locating it. A page is `private, no-cache` with
  an `ETag` that names its bytes, so one URL that names different pages for two
  accounts on one browser is revalidated, not served stale. A cover's `?v=`
  names its bytes and is cached as immutable, so the series cover's `?v=`, and
  any other version taken from page 0, is taken from the narrowed page 0.
  Nothing else the web keeps by work, edition and page outlives a change of
  account: signing in or out empties every read and unmounts the reader
  (`Gate.tsx`), and what survives is the HTTP cache above and per-browser
  preferences.
- **Downloads.** An edition download, the web's and OPDS's, zips the visible
  members only. A member download takes the ordinal of the narrowed document,
  and an ordinal past the visible members is refused as an ordinal the edition
  does not have.
- **Progress** is stored in the full numbering. A read translates it; a
  position on a hidden page is sent as the nearest visible page before it, or
  the first visible page when there is none before it. A write is checked
  against the narrowed slots and stored translated.
- **Notes** are stored in the full numbering. A read sends only the notes on
  visible pages, translated. A notes write replaces the reader's whole
  document, so a write from a narrowed account is translated back and keeps
  every stored note on a page it cannot see: without that, saving a note would
  delete the others.
- **Ground truth** is shared between accounts. A read leaves out a narrowed
  edition's entries; a write keeps the stored entries of a narrowed edition
  and is refused if it adds or changes one.
- **The notes and truth `ETag` names the numbering** (decision 11). For an
  account with grant limits it is taken over the stored document and the
  account's view of the work: the editions it sees, each narrowed edition's
  visible members in order, and the full slot each of its slots stands for.
  It is keyed by the `sha256` of every member the account is not sent: the
  full slots are small numbers, so a tag over them and what the account is
  sent alone could be matched by guessing them, giving a hidden member's
  size away. A write read under another view (a grant changed, or a scan
  changed the pages before a visible one) is refused with 412 and stores
  nothing, rather than being applied in the new numbering, where it would
  move notes and drop what the account had not been sent. For an account
  that sees every library the `ETag` is the stored document's, as before.
- **An alignment correction** that touches a narrowed edition is refused.
- **Browse** counts an edition's files, pages and missing files by the members
  the account sees (`browse::editions_by_work`), on a page opened with
  `?library=` too (decision 15), so a library page's counts and the work card's
  page count agree with the narrowed edition the account is served.

**Every route declares it.** Beside its grant posture
(`routes::grant_postures`), every route says what it does with a narrowed
edition (`routes::narrowing_postures`): translates, narrows, keeps what it
does not send, refuses, or reads no pages. A test drives each declaration against a narrowed fixture, so
a route added later cannot forget.

**O191 on the way.** O191's open part is the same keep-what-it-cannot-see rule
for an edition the account cannot see at all: the truth read sends that
edition's entries and a write replaces them, and the notes read sends the
reader's notes on it. The notes and truth rules above are applied to a hidden
edition too, which closes O191.

**The administrators' page.**

- `GET /api/library/split-editions`, refused to a non-administrator as the
  other configuration routes refuse one, computed from the store when it is
  asked: every edition that some library's directories hold part of but not
  all of. For each: the work, its title and the edition key; every member with
  its path, its page count and the libraries whose directories hold it; and for
  each library whose directories hold one of its members, the members and
  pages an account granted only that library sees.
- A fifth Configuration tab, "Split editions", shown to administrators only.
  One row per split edition, and under it one line per library ("sees 2 of 3
  files, 180 of 270 pages"); each directory links to its page. The tab says
  that an account granted every library listed sees every file a directory
  holds, and one that sees every library, as an administrator does, the whole
  edition, and how to split one: give one directory its own edition key (the
  directory's setting, or a rule), then scan it. With no case it says "No
  edition is split between libraries."
- Nothing is stored, and the menu shows no count.

## 4. PR 2: nested directories across libraries

- **`vetted`** refuses equal, inside and around only against the same library's
  directories (W). Across libraries, N is accepted. O204 is fixed while it is
  open: the vetting moves inside `create_library`'s and
  `add_root_checked_in`'s transaction.
- **Move** refuses a directory equal to, inside or around one the target
  library holds.
- **The walk.** An outer directory's walk skips any directory another root
  holds, so each file has one `scan_entry`, under its deepest root, and that
  root's rules and settings apply to it.
- **Adding an inner directory** takes over the files under it: the outer root's
  entries there are dropped and the inner root scans them. Their works keep
  their identity, which is content, and so their positions; what can change is
  which directory's rules name them, and with it their series.
- **Removing an inner directory** hands its files back to the directory around
  it, in the same operation: the outer subtree is rescanned before the inner
  root's `member_root` rows go, so nothing the outer directory still holds is
  counted as removed.
- **Reach.** `account::access_for` and `browse::library_scope` expand a
  library's directories to every directory nested inside them, by `root.path`
  prefix over the `root` table (canonical paths), leaving out a nested
  directory whose path the outer directory's `exclude_globs` match.
- **Configuration.** The refusal sentences say "of the same library". A
  directory's page says what its settings govern: an outer one names the
  nested directories it does not govern, an inner one names the library whose
  directory it sits in.

**What PR 2's plan settled that this section left open**, told to the user
rather than ruled (`docs/superpowers/plans/2026-11-04-overlapping-libraries-pr2.md`):

- **Grants decide which libraries an account lists and opens; reach decides
  which books it sees.** The libraries design of 2026-10-05 §3, "a restricted
  account sees only granted libraries", holds. An account granted Manga sees
  Kids' books on Manga's page, but its sidebar does not list Kids and
  `?library=` naming Kids does not open; an account granted Kids cannot open
  Manga's page or OPDS feed by id.
- **An equal directory stays refused across libraries until PR 3**, and W
  stays refused.
- **Reach is decided by the library's own directory alone**, asked as its walk
  asks: that directory's `exclude_globs` for each ancestor of the nested
  directory's path relative to it, outermost first, and the walk's rule that
  skips names starting with a dot. It does not chain through intermediate
  directories, and it is computed on every call, so a pattern edit changes it
  at once.
- **O204:** only the comparison with the stored roots (`scan::clash`, which
  replaces `vetted`) moves inside the transaction. Reading the disk stays
  before it.
- **A directory at or inside another directory's directory member is refused
  at add**: a folder of page images read as one volume would otherwise be
  counted without the nested directory's pages while the reader serves them.
  The refusal knows only what the last derive recorded (O246).
- **The walk compares a symlinked child directory by its canonical path**, so
  a link to a nested directory is skipped too, and rows recorded through one
  before the nested directory existed are forgotten.
- **Move's "who gains and loses sight" is computed by reach**, before and
  after the move.
- **A hand-over does what an in-root rule edit does, no better and no
  worse.** Positions stay where they were, and the series follows the inner
  directory's rules. An edition the inner's rules name differently is kept
  beside the new one, as the derive keeps an edition a rule edit renames, and
  is counted in `kept_editions`. A volume straddling the boundary leaves later
  positions in the old numbering (O244). None of this is fixed here.
- **The hand-over moves only the members of files the inner directory's scan
  recorded** (decision 21, replacing the plan's every member moving). A stale
  edition's members share a recorded file's path, so they move too. A file
  the inner directory's patterns exclude keeps its member with the outer
  directory, which drops the file's row.
- **The hand-back copies the inner directory's rows rather than walking the
  disk again**, since a walk cannot run inside the removal's transaction and
  the rehearsal counts without reading the disk. Each row goes to the
  directory whose walk would reach it: one nested in the removed directory,
  by that one's patterns, or else the deepest directory around it, by its
  own. A file the inner directory's patterns keep out has no row to copy.
  Kept out by the scan that took the files over, its member never left the
  outer directory; kept out later, its member goes where a row at its path
  would.
  Either way, where the outer's patterns admit the file, the outer's next
  scan records it again under the work it had. The member of a file gone
  from the disk while the inner directory's patterns still admit it is
  counted removed, as before.
- **An outer directory's file-level pattern governs nothing under a nested
  directory**, and governs the file again once it is handed back.

## 5. PR 3: the same directory in several libraries

- A migration (`019`) adds `library_root (library_id, root_id)` and drops
  `root.library_id`. `root.path` stays unique: one directory, one set of
  settings, linked to several libraries. `root_gets_a_library` is dropped,
  and the add names a directory's own library itself: a trigger on the link
  cannot fire for a directory that has none, and SQLite has no deferred
  triggers. A trigger refuses deleting a directory's last link while the
  directory stands. Nothing can refuse a directory committed with no link,
  so every writer makes the directory and its link in one transaction.
- Every reader of `root.library_id` reads the link instead. The web lists
  keyed by `root.path` alone key by library and directory.
- **Adding a directory another library holds** links it, rather than refusing.
- **"Remove from this library"** unlinks; it deletes nothing while another
  library still holds the directory. **"Remove directory"** removes it from
  every library, as removal does now. Removing a library removes its links,
  and a directory left with none is removed.
- **Move** becomes unlink and link.

**What PR 3's plan settled that this section left open**, told to the user
rather than ruled (`docs/superpowers/plans/2026-11-05-overlapping-libraries-pr3.md`):

- **The own library is named in Rust**, in the add, by the names
  `scan::own_library_clash` already works out. `root_gets_a_library` cannot
  become a trigger on the link: such a trigger never fires for a directory
  with no link, and SQLite has no deferred triggers. The code-point list
  duplicated between migration 016 and `trim_name` then lives in Rust alone.
- **The triggers are three.** `root_keeps_a_library` refuses deleting a
  directory's last link unless the directory itself is being deleted.
  `library_emptied_by_unlink` and `library_emptied_by_move` delete a library
  left with no link. A directory committed with no link cannot be refused,
  since a trigger at its insert cannot know whether a link follows, so each
  public writer is tested to leave every directory with a link.
- **Remove from this library refuses as Move does**: while the store's job
  lock is held, and while a scan is reserved in this library. So the page
  holds it exactly when it holds Move….
- **A link runs no watch probe and changes no setting.** The probe writes
  `scan_interval_secs`, which the libraries share. The add's answer says which
  libraries already held the directory.
- **New library over a directory another library holds links it**, as the add
  does.
- **`scan --add PATH` with no `--library`, over a directory another library
  holds, is refused as now.** It names no library to link to.
- **Move takes a required source library.** A default for a directory with one
  link would let a stale page move it out of a library it was linked to since.
- **The add asks W of the target library before it links.** Before PR 3,
  `scan::beside` returned at the first equal path, before it asked about
  nesting in the target, so a plain relaxation would have linked `/m/kids`
  into a library that holds `/m` whenever `/m/kids` had the lower id.
- **A removal that empties several libraries names each of them.** Its
  `left_with_no_library` is every limited account whose every grant is among
  them. Asking `only_grant_holders` of each library in turn would miss an
  account granted exactly those.
- **Removing a library unlinks its shared directories and removes the rest.**
  It clears the scan slots of the removed directories only, and it says which
  directories it leaves to which library ("…and leaves `/m` to Kids.").
- **The directory page says the other libraries once**, in its header: "Also
  in Kids, which shares these settings and scans." Each name links to that
  library's page for the directory. The wording avoids decision 17's "Also
  shown in", which means nested.
- **After a link, the add says** "`/m` is also a directory of Kids. It keeps
  the settings it has there." The line is drawn where the picker was.

## 6. Rulings this reverses or reopens

- **Reversed:** the libraries design of 2026-10-05, decisions 1, 2 and 3
  (PR 1); its §3 "a series is in exactly one library" (PR 1); its §6.2 Move
  steps (PR 1); its §10 "two libraries cannot share a folder, and one library's
  folder cannot sit inside another's" (PRs 2 and 3); its §2 "it now belongs
  to exactly one library" (PR 3); "nested roots refused in
  both directions" in the library configuration design of 2026-08-14 §2, the
  build design of 2026-09-02 and the C4 screens spec of 2026-09-04, now
  refused only within one library (PR 2).
- **Held:** O47, by keeping W refused by add and by Move; O220's no-library
  rule; O60; rule scopes (decision 4 of 2026-10-05).
- **Reopened:** O191 ("a reader granted one library is shown another library's
  work row") describes what decision 4 now intends: a work shows to an account
  that can see one of its editions. Its row is re-read in PR 1, and closed or
  narrowed to whatever remains a defect. What remained (the notes and truth
  routes sending, and truth replacing, a hidden edition's entries) is closed
  by §3.8.
- **Amended:** §1's "what an account sees", as first written, served a merged
  edition whole (O239); decision 6 narrows it (§3.8).
- **Fixed on the way:** O204 (PR 2).

## 7. Testing

- **Migration 018**: a store at schema 17 holding per-library copies of one
  work and same-title series in two libraries, with a row of every table that
  names a work or a series on each copy; after it, the lowest-id work and
  series stand with their rows, ids and slugs, each other copy is gone with
  every row naming it, positions and notes included, a work of a series that
  went is in the kept series by derivation and by hand, and a series cover
  naming a work that went is cleared (§3.1).
- **Access**: an account granted only Kids sees only the edition under Kids'
  directories of a work whose other edition is under Manga's; an account
  granted neither sees nothing; a copy held under two libraries' directories is
  visible through either grant (the case `held_member_meta`'s NULL fails
  closed today).
- **Copies (decisions 12–14)**: a copy whose two directories file it in
  different series is one work, held under both, named by the older directory
  in either order of adding them, and kept with its position when either
  directory is removed; a copy arriving under an older directory renames
  nothing; a volume sharing one of its files splits per file.
- **Nesting (PR 2)**: adding `/m/kids` leaves positions on its books where they
  were; removing it hands its files back to `/m` with nothing counted as
  removed; an `exclude_globs` match on the outer directory keeps a nested
  library's books out of the outer library; Move refuses W.
- **S (PR 3)**: removing a directory from one library leaves it, its books and
  their positions in the other.
- **Narrowing (PR 1, §3.8)**: core tests of the narrowed edition and its page
  map with the hidden member in the middle, so a map that translates nothing
  fails, a spread across the boundary, and a position on a hidden page. Serve
  tests over one edition of three members whose middle member is only under
  Kids, read by an account granted Manga: one test per route, the notes write
  keeping the hidden note, the truth write keeping the hidden entries. The
  administrators' route: an administrator is listed the case, anyone else is
  refused, a byte-identical copy under both libraries is not a case, and no
  case gives an empty list. The tab: its rows, its empty state, and hidden from
  a non-administrator. Then a re-drive: Task 10b's scratch store served again,
  read end to end as an account granted one library.
- Each test is watched failing against the defect it names, applied by hand,
  before it is trusted.
- Each PR is driven on a scratch copy of the test library; PR 2's and PR 3's
  configuration pages go on a live server for the user to try before review.

## 8. What this does not do

- No per-library series names: a book has one series, named by its deepest
  directory's rules.
- No library scope for rules (decision 4 of 2026-10-05 stands).
- The symlink defect in §2 is its own row.

## 9. What could not be settled from here

- **How many copies the NAS database holds.** Settled by the user's ruling of
  2026-11-02 that there is no fold (§3.1): migration 018 reduces any number of
  copies to the oldest row and reports nothing, so the count changes nothing.
- **Hand documents of de-duplicated copies.** Each copy can have its own
  `<root>/.taiyaku/<content_key>.library.json`. Settled by the same ruling: the
  kept work's document stays at its address, a deleted copy's file is left on
  disk untouched, where `hand::reconcile` reports it as an orphan, and the
  deleted copy's hand values go with its row.

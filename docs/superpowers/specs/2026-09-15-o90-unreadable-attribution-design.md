# O90 — the sniffer knows eight containers, the reader admits two: build design

**Status: settled in brainstorming on 2026-09-15; amended 2026-09-16, before
any code, by a pre-flight over the code and the real library.** The amendments
are made in place in the sections they change and gathered here so none is
missed:

1. **The person decided that the files are named** (2026-09-16). A file that
   would not count and a file that vanished become lists of paths; a refused
   format and a container holding no pages stay counts. §2.1 says why the
   split falls there.
2. **The spec missed the API and the web screen.** `config_api.rs`'s
   `derive_report_json` renders `"unreadable"`, and `web/src/ScanReport.tsx`
   draws it in Settings. That is where a person using taiyaku reads a scan,
   so a breakdown the CLI alone prints would not reach them. §2.4.
3. **The real library's thirteenth unreadable entry did not vanish.**
   `Bisque Doll de Chu ♡.cbz` is a zip that opens cleanly and holds 185
   JPEG XL (`.jxl`) pages, a format `taiyaku-read`'s `IMAGE_EXTENSIONS`
   does not list. So `member_pages` refused it, the walk recorded
   `pages IS NULL`, and derivation counted it with the containers that would
   not open. O90's row and §0.1 below said it was a member whose bytes could
   not be hashed. §0.1 and §2.2 are corrected.
4. **So "could not list" was the wrong name for the NULL case.** The one real
   instance lists fine. A NULL page count means the walk could not *count*
   pages, because the archive would not open or because it held no page image
   taiyaku reads. The field is `uncounted`, not `unlistable`.
5. **`refused_format` is a method, not a field.** It is the sum of
   `by_format`, and two stored numbers that must agree are two numbers that
   can disagree.
6. **Corrected by the pre-merge claim check, 2026-09-17**, each in place:
   Vagabond was not wholly invisible (§0); the repack took seconds, not ten
   minutes (§1); `unfoldable`'s doc gives a different reason from
   `unreadable_hand`'s, and the scale figure was the archive count (§2.1); the
   example line's proposal count (§2.2); `config.ts`'s "thirty" was corrected
   after all (§2.4); a byte concatenation does not change by an empty entry,
   and the repack's size is 4.52 GiB, not GB (§4); two §5 bullets now
   describe the tests as built; and the mutation harness's `Default` blind
   spot is a type deriving *no* `Default`, which hides no function this branch
   changes (§6).

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O90**. It does not close it by teaching `taiyaku-read` to open a
RAR — §1 is the record of why not, and that refusal is half the work, because
the row's own complaint is that *"nothing anywhere records that gap as
deliberate."*

The reader's format layer is `crates/taiyaku-read/src/format.rs`; the walk is
[`2026-08-14-library-scanner-design.md`](2026-08-14-library-scanner-design.md).

## 0. The gap in one paragraph

`taiyaku_read::sniff` answers what a container is from its first 512 bytes and
names eight of them. `Format::is_readable` admits two, `Dir` and `Zip`.
`derive::is_readable` spells the same two in `scan_entry.format` tokens,
because it holds a token and there is no parse back to the enum.
`derive::admit` counts a refused format into `DeriveReport::unreadable` and
returns `None`, so the entry never becomes a member and the only trace it
leaves is one integer shared with three other outcomes. Measured over a real
library on 2026-09-01: 13 unreadable, of which 12 were volumes of one series —
Vagabond's volumes 3–12, 19 and 20, stored as RAR archives named `.cbz`,
invisible while its other 25 volumes read.

## 0.1 What O90's row gets wrong, corrected here rather than quietly

The row says the field *"has a second site"* and calls it **one counter, two
causes**. `DeriveReport`'s own doc comment already says four, and there are
three increment sites:

| Situation | Site |
| --- | --- |
| A format `Format::is_readable` refuses | `admit`, `derive.rs:1088` |
| A container whose pages the walk could not count (`pages` is NULL) | `classify`, `derive.rs:1189` |
| A container that listed cleanly and held no page images (`pages` is `Some(0)`) | `classify`, `derive.rs:1189` |
| A member whose file was no longer there to hash | `classify`, `derive.rs:1193` |

The middle two share one `let ... else`, because `entry.pages.filter(|p| *p > 0)`
answers `None` for both. So the taxonomy was written down correctly and the
*report* is what collapses it. The row's "two causes" describes the 2026-09-01
run and not the field. Corrected here because a spec that repeated it would
have built two counters where four are needed.

**And the row's second cause was wrong too** (amendment 3). This section first
said the run held "12 refused formats and 1 vanished member", repeating the
row. Measured read-only on 2026-09-16 against
`/home/davey/temp/manga-test/library.db`, the one entry still unreadable after
the repack is `Volumes/Bisque Doll de Chu ♡/Bisque Doll de Chu ♡.cbz`:
`format = 'zip'`, `pages IS NULL`, `sha256 IS NULL`. `classify` tests `pages`
before it hashes anything, so that entry is counted by the first `let … else`
and never reaches `digest`. It is not a vanished member. The archive itself
lists cleanly under `bsdtar -tf` and `python3 -m zipfile -l`, and
`zipfile.testzip()` finds no bad entry. All 185 of its entries are `.jxl`,
and JPEG XL is not in `taiyaku-read`'s `IMAGE_EXTENSIONS`. So `member_pages`
takes `read_member`'s "no page images" refusal, and `pages_of` swallows it.

**For a file, `pages = Some(0)` does not occur.** `member_pages` turns an
archive with no page images into an error, never a zero, so the third row of
the table is reached only by a directory, and the second covers two different
facts about a file: an archive that would not open, and one that opened and
held nothing taiyaku reads as a page. The walk keeps no record of which, so
derivation cannot split them. Recording the refusal would be a schema change,
and §3 rules that out for this item.

## 1. What this deliberately does not do: add a RAR decoder

**Decided 2026-09-15.** taiyaku is `GPL-3.0-or-later` (`Cargo.toml:16`). The
three candidates and what each cost:

- **`unrar`** — an MIT/Apache Rust wrapper that builds RARLAB's C++ UnRAR from
  bundled source. That source carries Alexander Roshal's restriction: it may
  not be used to re-create the RAR compression algorithm. Fedora classifies
  the UnRAR licence as GPL-incompatible and non-free, and calibre carried the
  same bug for years. GPL forbids a distributor adding restrictions, so a
  GPLv3 work cannot carry it. **Ruled out on licence, not on quality.**
- **`unrar-rs`** — declares `GPL-3.0-or-later` *and* states it incorporates
  UnRAR source bound by Roshal's restriction. Those two cannot both be true of
  one artefact. **Ruled out because a dependency whose own licence
  contradicts itself is a liability**, and the contradiction is invisible to
  any audit that reads only the declared field.
- **`rar`** — clean-room, `nom`-based, MIT, and therefore the only
  licence-clean route. **Ruled out on maturity, and only for now**: 403
  downloads a month, v0.4.0, and "limited functionality" in its own words.
  `taiyaku-read` is the crate every page read passes through, and an immature
  decoder there is a worse risk than twelve files are worth.

**The alternative that won is not a code change at all**: the twelve files are
repacked to zip (§4), which took seconds and costs nothing afterwards.

**This paragraph is the artefact.** The row's complaint is that nothing
records the gap as deliberate; §1 and the doc-comment change in §2.3 are that
record. Should the `rar` crate mature, the decision to revisit is this
section, and what would reopen it is a library where repacking is not
available — read-only media, or a share someone else owns.

## 2. What changes

### 2.1 `DeriveReport::unreadable` becomes four causes, two of them named

*As amended 2026-09-16 (amendments 1, 4 and 5).*

```rust
/// Why derivation could make no structure out of an entry, one field per
/// cause.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unreadable {
    /// Entries in a format `taiyaku_read::Format::is_readable` refuses, by
    /// `scan_entry.format` token (`unknown` for a NULL format).
    pub by_format: BTreeMap<String, usize>,
    /// Containers whose pages the walk could not count (`pages` is NULL),
    /// each named by its path under its root.
    pub uncounted: Vec<String>,
    /// Containers that listed cleanly and held no page images of their own.
    /// A volume directory holding archives is one; the archives inside it
    /// are what become the members.
    pub no_pages: usize,
    /// Members whose file was no longer there to hash, each named by its
    /// path under its root.
    pub vanished: Vec<String>,
}

impl Unreadable {
    pub fn refused_format(&self) -> usize; // by_format's values, summed
    pub fn total(&self) -> usize;          // all four causes
}
```

`unreadable: usize` becomes `unreadable: Unreadable`, and `total()` gives
every caller that compared one number a number to compare.

**A path where a person must find the file, a count where they need not.**
`unreadable_hand` is a `Vec<String>` whose doc comment gives the reason: *"the
person has to be told **which** file to fix."* (`unfoldable` is one too, for a
neighbouring reason: a count says something was refused and not what to look
at.) The first version of this spec made all four causes counts, arguing from
scale. `unreadable_hand` holds hand documents, of which a library has a
handful, while this field counts scan entries, of which the 2026-09-01 library
has 2,044, 1,915 of them archives. The argument holds for two of the four causes and not for the
other two:

- **A refused format stays a count, per format.** Twelve RAR archives are
  repacked by one loop, and their names add nothing to that. `by_format` is
  bounded by the tokens `Format` has, not by the size of the library, so the
  scale argument is sound here.
- **A container holding no pages stays a count.** Its usual instance is a
  volume directory holding archives, which is a layout rather than a fault.
  There is nothing to fix, and a list of every such directory would bury the
  entries that do need fixing.
- **An uncounted container is named.** The one real instance is a single
  archive of JPEG XL pages. "1 uncounted" alone leaves the person querying
  the database for it, and the person asked for the file to be named.
- **A vanished member is named**, for the same reason. It is one file that
  was there at the walk and gone at the derive.

The scale worry does not vanish; it moves. A share unmounted between the walk
and the derive could name every member under `vanished`. That is the same
exposure `ScanReport::unreadable_dirs` already accepts, and it names a real
fault truthfully.

**Paths are joined to their root**, `root.path.join(rel_path)`, because a
library of two roots can hold the same `rel_path` under both, and
`unreadable_hand` and `unreadable_dirs` already name files by their full path.

### 2.2 The scan line says which

`derive_summary` in `crates/taiyaku-cli/src/library.rs` prints `unreadable`
only when non-zero, which stays. What changes is that the number is followed
by its causes, each named only when non-zero, the convention `scan_summary`
already follows for `unreadable_dirs` and `excluded_dirs`:

```
derived 130 series, 1858 works, 1858 editions, 1883 members,
  1 unattached chapter directories, 13 unreadable (12 rar,
  1 whose pages could not be counted: /…/Bisque Doll de Chu ♡.cbz),
  25 merged, 1 series holding no work; 29 proposal(s) raised
```

*(Amended 2026-09-16. The example first read `(12 rar, 1 vanished)`; the
thirteenth entry was the JPEG XL archive, per §0.1.)* A path list joins on `; `,
the separator every other list on this line already uses, and the causes are
separated by `, ` like the line's own clauses.

The format tokens are the schema's own vocabulary and print verbatim, for the
reason `Format::token`'s doc gives: they are not free text, and a friendlier
spelling here would be a second name for a value the database already has one
name for.

### 2.3 `Format::is_readable` records the gap as deliberate

Its doc comment gains what §1 decided: that the sniffer names more containers
than the reader admits, that this is a decision rather than an oversight, that
the reason for RAR specifically is licence and not difficulty, and where the
record is. This is the half of O90 that is not a counter.

### 2.4 The API and the Settings screen say which too

*Added 2026-09-16 (amendment 2).* `config_api.rs::derive_report_json` is the
one renderer of `DeriveReport` for the scan status, a rule apply and a root
removal alike. It destructures with no `..`, and its doc gives the reason: a
field cannot go missing on the way out. `"unreadable"` becomes an object,
rendered by its own helper under the same rule, holding every field of
`Unreadable` plus `refused_format` and `total`. Those two are computed by the
server so the client does not re-implement the sum.

`web/src/config.ts`'s `Derived.unreadable` becomes that object's type, and
`ScanReport.tsx`'s `DeriveSection` draws the total and then each cause in
turn: the per-format counts, the two path lists and the no-pages count. It
uses the existing `Count` and `Paths` shapes, and every cause is drawn even
when it is zero, which is that file's own rule. `DeriveReport` keeps its
thirty-one top-level fields, so no count of them moves. (`config.ts`'s doc
said "thirty", one short since O76; it and three other comments that said the
same were corrected on this branch.) The test that counts `data-field` elements rises by the causes drawn.

## 3. What this does not change

Derivation still skips these entries, and the twelve files stay unreadable to
taiyaku until they are repacked. Nothing in `taiyaku-read`, the rules engine,
the scanner's walk, or the schema changes. `scan_entry.format` already records
`rar` correctly and always did.

## 4. The data fix, which is not part of the PR

The twelve files are repacked from RAR to zip in place. `bsdtar` reads RAR and
writes zip, and is installed here (`/usr/sbin/bsdtar`), so the conversion is
one command per file and needs no decoder in taiyaku.

**This writes into the library tree**, so it is done deliberately and
separately from any code change, and each file is verified against its source
before it replaces anything.

**Done on 2026-09-15, and the figures are measured rather than predicted.**
All twelve converted with `bsdtar -c --format zip --options
zip:compression=store @<archive>`, which streams archive to archive and needs
no staging directory. The RAR payloads turned out to be *stored* rather than
compressed, so 4.52 GiB converted in seconds. A rescan then reported:

| | before | after |
| --- | --- | --- |
| `unreadable` | 13 | **1** |
| works | 1858 | 1870 |
| members | 1883 | 1895 |
| `merged` | 25 | 25 |

Vagabond went from 25 works to 37 — volumes 3–12, 19 and 20 are the twelve
that were invisible. `merged` is unchanged, which is the control: repacking
must not touch O89's case, and it did not.

**Two of the twelve failed their first verification, and the verification was
what was wrong.** The check compared the concatenation of every entry's bytes
and the sorted list of entry names. `Vagabond - v19.cbz` and
`- v20.cbz` store their pages under a directory entry, which RAR records
*without* a trailing slash and zip writes *with* one — so the name lists
differed by one character on an entry that is not a page. 178 and 198
real files respectively were byte-identical the whole time. Re-verified per
entry with directory entries excluded by type rather than by name, both
passed. **Neither file was replaced while its check was failing**, which is
the property the loop was built for: convert to a temporary beside the
original, verify, and only then rename over it.

A page count alone would not have caught a genuine failure here, which is why
the check is per-entry content and not a count.

**Repacking changes every file's `sha256`, and therefore the work's
`content_key`.** That is O81's condition — a hand document addressed by
content key is stranded when the hub edition's files change. It costs nothing
here, and the reason is measured rather than assumed: on 2026-09-15 the
library at `/home/davey/temp/manga-test/library.db` held **0 rows in
`progress` and 0 in `note`**. A library with reading history would need O81
considered first, and this paragraph is here so that a person repeating these
steps later checks rather than inherits the answer.

## 5. Tests

- Four fixtures, one per situation, each asserting its own field moved and the
  other three did not. The refused-format fixture is a real `Rar!\x1a\x07`
  header in a file named `.cbz`, which is O90's own case.
- `by_format` names `rar` for that fixture, and holds one entry per refused
  format when a library has several: the test built holds `7z`, `rar` and
  `unknown`.
- `total()` sums every cause once and `refused_format()` sums `by_format`'s
  counts, both asserted over a synthetic `Unreadable` with a different count
  per cause; the fixtures compare the whole struct by equality instead of
  calling `total()`.
- `derive_summary` prints the causes when non-zero and omits each when zero —
  asserted per cause rather than against one whole-line string, since a
  whole-line `contains` cannot see which cause was omitted.
- `the_token_side_readability_test_agrees_with_the_enums_own` is unchanged and
  keeps `derive::is_readable` and `Format::is_readable` in step.

*Added 2026-09-16, with amendments 1–5:*

- The four fixtures assert the whole `Unreadable` by equality, so every field,
  the path lists included, is pinned at once rather than one at a time. The
  uncounted and vanished paths are asserted as full paths under the root.
- An archive holding only `.jxl` pages is counted uncounted and named: the
  real library's case, reproduced.
- `refused_format()` and `total()` each have a test with a distinct number in
  every cause, so a sum dropping any one term changes the answer.
- `derive_report_json` renders every key of `unreadable` with its value,
  asserted key by key on a report whose causes all differ.
- `DeriveSection` draws each cause as its own element with its label and its
  value, asserted per element, since a whole-page `toContain` cannot see a
  label paired with the wrong number.

## 6. Risks

- **`DeriveReport` is `PartialEq` and constructed in tests.** Changing
  `unreadable`'s type touches every test that builds or compares one. That is
  mechanical, and the compiler finds all of it.
- **A clean sweep says less than it appears to.**
  `docs/mutants-baseline.md` records (O88) that a function returning a type
  that derives *no* `Default` loses its `Default::default()` whole-body
  mutant, and one with no other viable mutant then goes unseen. No function
  this branch changes is in that position: `Unreadable` derives `Default`, and
  `admit`, whose `Some(Default::default())` mutant cannot build, keeps a
  viable `None` mutant, which the sweep caught. The four per-cause
  tests are what actually guard the split, and each must be checked by hand
  against its own mutation — set the wrong field and watch the test fail.
- **A `BTreeMap` field in a `PartialEq` struct makes an empty map and an
  absent key the same fact**, which is correct here and worth stating: a
  format that refused nothing has no entry rather than a zero.

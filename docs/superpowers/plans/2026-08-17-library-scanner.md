# The library scanner — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill the library from directories on disk — a persisted layer of facts about what is there, an ordered and user-editable rule layer that turns those facts into series, works, editions and members, proposals for the judgements §1's measurement says no rule should make, and change detection that reports per root what it can actually do.

**Architecture:** Facts and rules are two tables and two passes. `scan` writes `scan_entry` and `dir_state` and decides nothing; `derive` reads `scan_entry` + `rule` and writes structure through the existing `import` path, so `work`, `edition` and `member` keep exactly one writer. Because the facts are persisted — page counts and digests included — editing a rule re-derives the whole library without touching the filesystem, which is §2's claim and what Task 7 tests by deleting the tree first.

**Tech Stack:** Rust 2024, `rusqlite`, and two new dependencies in `taiyaku-library`: `regex` (§4's escape hatch) and `notify` (§8's watcher). Tests are `cargo test` under `scripts/check-all.sh`.

**Spec:** `docs/superpowers/specs/2026-08-14-library-scanner-design.md`. Read it before Task 1. Every `§n` below points at it; the store spec is named in full where it is meant.

## Global Constraints

- **Edition 2024, `rust-version` 1.85.** Both come from `[workspace.package]`; do not set them per crate.
- **`unsafe_code = "forbid"`** workspace-wide. It applies to our crates, not to dependencies.
- **`taiyaku-library` must never be reachable from `taiyaku-core` or `taiyaku-wasm`.** Those compile to `wasm32` and neither SQLite nor `notify` runs there. This is why Tasks 1 and 2 put the shared pieces in `taiyaku-read` and not in `taiyaku-core`.
- **Migrations are append-only.** Migration 3 is a new file. Never edit `001_initial.sql` or `002_absorbed.sql` — a database already at version 2 is never shown them again, and the two would diverge silently. `SCHEMA_VERSION` and `MIGRATIONS.len()` are equal by a `const` assertion, so bump both.
- **Tests live in a sibling file**, `src/<module>/tests.rs`, declared with `#[cfg(test)] mod tests;` at the bottom of the module. This is the convention across all six crates as of PR #74.
- **Every `Result`-returning `pub` function needs an `# Errors` doc section.** `clippy -D warnings` runs on `--all-targets --all-features`, pedantic enabled.
- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.** Run it before every commit that closes a task. Never run it with `TAIYAKU_CORPUS` set — the script exports it into an unfiltered `cargo test --workspace`, which runs `geometry_experiment` in debug and takes over an hour.
- **The scanner reads.** §10: it never writes to the library tree. The one exception is §8's watcher probe, which writes a dot-prefixed temporary file into a root and removes it; it is the exception because the question it answers cannot be answered any other way, and it is named at every point it appears.
- **No attribution trailers in commit messages.** No `Co-Authored-By`, no `Generated with`.

---

## Four corrections to the spec, found by reading the code

The spec was settled in brainstorming against the design documents, not against the call sites. Four of its statements do not survive contact with the tree as it stands on 2026-08-17. Each is settled below in the task that meets it; they are collected here so a reader of the spec alone is not surprised.

1. **§4 calls `inventory::volume_number` "called as a matcher". It cannot be.** `inventory` lives in `taiyaku-cli`, and the dependency edges run `taiyaku-core → taiyaku-read → taiyaku-library → taiyaku-serve → taiyaku-cli`. A rule engine in `taiyaku-library` cannot see it. Task 1 moves it down to `taiyaku-read::naming`, beside `natural_cmp` and `is_page_name`, which is what keeps the spec's actual promise — one volume parser rather than two.

2. **§4's "and its chapter sibling" does not exist.** `rg 'chapter_number|fn chapter' crates/` finds only `Sidecar::chapter_at`, which answers "which chapter is this slot in" from an alignment and parses nothing. `builtin-chapter` needs a parser written from scratch, and it needs a decision `volume_number` did not: **no fallback to the first digit run.** `volume_number` falls back, which is right for a directory that is a volume by construction; a chapter parser that fell back would give every volume file a chapter number and every rule 3 would fire on everything.

3. **§3's format list needs a sniffer that does not exist.** `taiyaku-read`'s `unsupported_container` is private, returns English prose for an error message (`"a RAR archive"`), and knows nothing of `pdf`, `epub-fixed` or `epub-reflow` — three of the eight values `scan_entry.format` is specified to hold. Task 2 adds the public one and rewrites the private one on top of it, so there is one sniffer rather than two.

4. **A scanned work has no sidecar file, and `import::sidecar` requires a path.** `import::sidecar(conn, doc, path: &Path)` writes `work.sidecar_path` from that argument unconditionally (`import.rs:360`), and uses it as the second identity probe after `content_key`. The corpus convention is one `taiyaku.json` per work directory (`docs/sidecar.md`), while a scanned series directory holds fourteen volumes as *files* — so there is no per-work path to claim and no honest value to write. Task 3 gives `import` a path-less entry point rather than letting the scanner insert `work` rows itself, because a second writer of `work`/`edition`/`member` is exactly what the store spec's §4.2 exists to prevent.

**A fifth thing is not a correction but a collision.** The auth spec (`2026-08-14-library-auth-design.md`, §3) calls itself "Migration 2". `SCHEMA_VERSION` is already 2. Whichever of B and A2 lands second renumbers; this plan takes **003** and A2 becomes 004. Nothing else about either spec changes.

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-read/src/naming.rs` | `natural_cmp` (existing), plus `volume_number` and `chapter_number` — the one place a number is read out of a name |
| `crates/taiyaku-read/src/format.rs` | `Format` and `sniff`: what a container is, from its bytes |
| `crates/taiyaku-library/src/schema/003_scanner.sql` | `scan_entry`, `dir_state`, `rule`, `proposal`, and the four seeded default rules |
| `crates/taiyaku-library/src/scan.rs` | the walk and the fact layer: rows about what is on disk, no decisions |
| `crates/taiyaku-library/src/rules.rs` | the rule engine: four kinds, ordered layering per field, provenance |
| `crates/taiyaku-library/src/derive.rs` | `scan_entry` + `rule` → series, works, editions, members, through `import::scanned` |
| `crates/taiyaku-library/src/proposals.rs` | the four proposal kinds, the scoring vocabulary, accept-writes-a-rule |
| `crates/taiyaku-library/src/watch.rs` | §8's probe, the watcher, the interval, and the per-root report |
| `crates/taiyaku-library/src/import.rs` | modified: a path-less entry point |
| `crates/taiyaku-cli/src/library.rs` | modified: the `scan` and `rules` subcommands |

---

## Task 1: `volume_number` moves down, and `chapter_number` is written beside it

§4 wants both callable from `taiyaku-library`. One is in the wrong crate and the other does not exist. Doing both in one task is deliberate: they share a module, a test file and one design decision (whether to fall back), and splitting them would put that decision in a task that cannot see the function it contrasts with.

**Files:**
- Modify: `crates/taiyaku-read/src/naming.rs`, `crates/taiyaku-read/src/lib.rs`
- Modify: `crates/taiyaku-cli/src/inventory.rs` (delete the function, call the moved one)
- Test: `crates/taiyaku-read/src/naming/tests.rs`, `crates/taiyaku-cli/src/inventory/tests.rs`

**Interfaces:**
- Produces: `taiyaku_read::volume_number(name: &str) -> Option<u32>` — the existing function, verbatim, in a new home
- Produces: `taiyaku_read::chapter_number(name: &str) -> Option<f64>`

- [ ] **Step 1: Confirm the call sites before moving anything**

```bash
rg -n 'volume_number' crates/
```

Expected, on 2026-08-17: the definition at `taiyaku-cli/src/inventory.rs:35`, **two** callers — `inventory.rs:350` and `corpus.rs:658` — and the tests in `inventory/tests.rs`. `inventory/tests.rs:339` is a test *name* containing the words, not a call. If that is not what comes back, the move is larger than this task and the list below is stale — write down what you actually found before continuing.

*This step said "one non-test caller" until the first implementer ran the grep and found the second. It is kept as written rather than quietly corrected, because the reason it was wrong is the point: the count came from a `grep -rn 'volume_number' crates/ --include=*.rs`, which this shell is `zsh` and rejects outright with `no matches found: --include=*.rs` — and an errored grep printing nothing reads exactly like a clean one. `corpus.rs:658` sits inside a `TAIYAKU_CORPUS`-gated `#[test]`, so it also compiles only under `--all-features` and would not have surfaced in an ordinary `cargo build`.*

- [ ] **Step 2: Write the failing test for `chapter_number`**

Create `crates/taiyaku-read/src/naming/tests.rs` if it does not exist; otherwise append. The two tests that carry the whole design:

```rust
#[test]
fn a_chapter_number_is_taken_from_the_marker_that_precedes_it() {
    assert_eq!(chapter_number("c001"), Some(1.0));
    assert_eq!(chapter_number("Series - c014.cbz"), Some(14.0));
    assert_eq!(chapter_number("ch. 7"), Some(7.0));
    assert_eq!(chapter_number("chapter 128"), Some(128.0));
    assert_eq!(chapter_number("第7話"), Some(7.0));
}

#[test]
fn a_chapter_number_carries_its_decimal_and_never_falls_back() {
    // `member.chapter_declared` is REAL because half-chapters exist.
    assert_eq!(chapter_number("c001.5"), Some(1.5));
    // THE decision of this task. `volume_number` falls back to the first
    // digit run, which is right for a directory that is a volume by
    // construction. A chapter parser that fell back would answer for every
    // volume file in the library — `Series - v01 (2023).cbz` would carry
    // chapter 1 — and §4's rule 3 would then set a chapter on everything.
    assert_eq!(chapter_number("Series - v01 (2023).cbz"), None);
    assert_eq!(chapter_number("069"), None);
    assert_eq!(chapter_number("[2015] ONE PIECE v077"), None);
}
```

- [ ] **Step 3: Run it to make sure it fails**

Run: `cargo test -p taiyaku-read naming::tests::a_chapter_number`
Expected: FAIL — `cannot find function 'chapter_number' in this scope`.

- [ ] **Step 4: Move `volume_number` and write `chapter_number`**

Cut `volume_number` — the function and its whole doc comment — out of `crates/taiyaku-cli/src/inventory.rs` and paste it into `crates/taiyaku-read/src/naming.rs`. **Move it verbatim except for one paragraph**, which becomes false the moment Task 4 lands: the doc comment ends with

> Deliberately not a regex. `regex` is three crates in a workspace whose short dependency list `docs/status.md` states as a fact about the project, to do a job a scan does; `--pattern` covers what the default misses. If a collection turns up that neither handles, that is evidence for the dependency and it can be added then.

Replace that paragraph with:

```rust
/// Deliberately not a regex, and it stays that way now that the workspace
/// has one. `regex` arrived in `taiyaku-library` with the scanner's rule
/// layer on 2026-08-17 — the evidence the paragraph this replaces asked for
/// — but this function is what the scanner's `builtin-volume` rule *calls*
/// rather than something a pattern replaces. The reason is in that spec's
/// §4: this is hand-written, measured on a 579-directory One Piece run, and
/// carries a guard a regex in a text box will not, since a year *before* the
/// marker (`[2015] ONE PIECE v077`) must not be read as the volume.
/// Re-expressing it as a default regex would be a second implementation of a
/// measured behaviour, and it would drift.
```

Then add, below it:

```rust
/// The chapter number in a name.
///
/// Takes the first digit run — with an optional decimal part — immediately
/// preceded, optionally across one space or a `.`, by `c`, `ch`, `chp`,
/// `chapter` or `第`. **There is no fallback**, which is the one place this
/// deliberately differs from [`volume_number`] directly above.
///
/// `volume_number` falls back to the first digit run when no marker appears,
/// and that is right for its caller: `inventory` hands it a directory that is
/// a volume by construction, so a bare `069` is a volume number. A chapter
/// parser with the same fallback would answer for every volume file in a
/// library — `Series - v01 (2023).cbz` would carry chapter 1 — and the
/// scanner's `builtin-chapter` rule (§4) would then set `chapter` on
/// everything it saw. A chapter number requires its marker.
///
/// `f64` rather than `u32` because `member.chapter_declared` is `REAL` and
/// half-chapters exist: `c001.5` is one file in the corpus's chapters root.
/// Leading zeros are stripped by the parse, so `c001` and `c1` agree.
///
/// `第` matches both `第7話` and `第7巻`; the volume form is not excluded
/// here, because a name carrying an explicit volume marker is answered by
/// `volume_number` and a rule that sets both fields from one name is the
/// layering model working as designed rather than a conflict.
///
/// **The dotted forms are listed as markers rather than trimmed**, which is
/// how [`volume_number`] above already handles `vol.`: its `MARKERS` carries
/// both `"vol."` and `"vol"`. `Ch. 7` has *two* separators between the marker
/// and the digit, and the single `strip_suffix` below removes one — so `ch.`
/// has to be a marker in its own right.
///
/// **A marker must start at a word boundary, and that guard is doing real
/// work.** `c` and `ch` are one and two characters, so without it they match
/// the *tail of ordinary words*: `Scan by ABC 2019` reads as chapter 2019
/// because `abc` ends in `c`, and `which 7` reads as chapter 7 because
/// `which` ends in `ch`. Both were measured on the built function rather than
/// reasoned about. This is the same hole the no-fallback rule above exists to
/// close, reached by a different route — a name with no chapter in it getting
/// a chapter number — so closing one without the other would have been half a
/// decision.
///
/// The boundary test is ASCII-only on purpose: `第7話` must keep working when
/// it follows a kanji, and `char::is_alphanumeric` would reject that, since
/// every kanji is alphanumeric to Unicode.
pub fn chapter_number(name: &str) -> Option<f64> {
    const MARKERS: [&str; 8] = ["chapter", "chp.", "chp", "ch.", "ch", "c.", "c", "第"];

    let chars: Vec<char> = name.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && chars[at].is_ascii_digit() {
            at += 1;
        }
        // One decimal part, and only when a digit follows the point — so
        // `c001.cbz` reads as 1 rather than tripping over the extension.
        if chars.get(at) == Some(&'.')
            && chars.get(at + 1).is_some_and(char::is_ascii_digit)
        {
            at += 1;
            while at < chars.len() && chars[at].is_ascii_digit() {
                at += 1;
            }
        }
        let number: Option<f64> = chars[start..at].iter().collect::<String>().parse().ok();
        let before = chars[..start].iter().collect::<String>().to_lowercase();
        let before = before.strip_suffix([' ', '.']).unwrap_or(&before);
        // `strip_suffix` rather than `ends_with`, because the head it hands
        // back is exactly what the word-boundary test needs: the character
        // before the marker, or nothing at all.
        if MARKERS.iter().any(|marker| {
            before.strip_suffix(marker).is_some_and(|head| {
                head.chars()
                    .next_back()
                    .is_none_or(|c| !c.is_ascii_alphanumeric())
            })
        }) {
            return number;
        }
    }
    None
}
```

And two tests for the hole, beside the two already given:

```rust
#[test]
fn a_marker_must_start_a_word_rather_than_end_one() {
    // Both measured on the built function, not reasoned about: `c` and `ch`
    // are one and two characters, so without a boundary test they match the
    // tail of ordinary words. This is the no-fallback rule's hole reached by
    // another route — a name with no chapter in it getting a chapter number.
    assert_eq!(chapter_number("Scan by ABC 2019"), None);
    assert_eq!(chapter_number("which 7"), None);
    assert_eq!(chapter_number("Sonic 3"), None);
}

#[test]
fn a_marker_after_a_kanji_still_counts() {
    // The boundary test is ASCII-only for this: every kanji is alphanumeric
    // to Unicode, so `char::is_alphanumeric` would reject the one form of
    // this that is not English.
    assert_eq!(chapter_number("単行本第7話"), Some(7.0));
}
```

In `crates/taiyaku-read/src/lib.rs`, widen the re-export:

```rust
pub use naming::{chapter_number, natural_cmp, volume_number};
```

At the bottom of `naming.rs`, if it is not already there:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 5: Point `inventory` at the moved function**

Both callers move. In `crates/taiyaku-cli/src/inventory.rs`, the call at what was line 350 becomes `taiyaku_read::volume_number(&name)`, or add `use taiyaku_read::volume_number;` at the top and leave the call as it is. In `crates/taiyaku-cli/src/corpus.rs:658`, `crate::inventory::volume_number(&name)` becomes `taiyaku_read::volume_number(&name)` — it is inside a `TAIYAKU_CORPUS`-gated `#[test]`, so `cargo build` will not catch it if you miss it and `cargo clippy --all-targets --all-features` will. `taiyaku-cli` already depends on `taiyaku-read`.

Move the `volume_number` tests out of `crates/taiyaku-cli/src/inventory/tests.rs` and into `crates/taiyaku-read/src/naming/tests.rs` **unchanged** — they are the measured behaviour, and rewriting them while moving them is how a measured behaviour turns into an asserted one. `inventory/tests.rs` keeps its `Pattern` tests, which test a CLI flag and stay.

- [ ] **Step 6: Run both crates' tests**

Run: `cargo test -p taiyaku-read -p taiyaku-cli`
Expected: PASS, including every moved `volume_number` test.

- [ ] **Step 7: Gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "One volume parser, moved where the scanner can call it, and a chapter parser that does not fall back"
```

---

## Task 2: `taiyaku_read::sniff` — what a container is, from its bytes

`scan_entry.format` is specified to hold `zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z | NULL`. Nothing in the workspace answers that question: `unsupported_container` is private, returns prose for an error message, and covers three of the eight. §3 is explicit that format is sniffed by content and never by extension — the reason is in `README.md`, that a zip is often named `.cbr` and a real RAR often `.cbz`.

**Files:**
- Create: `crates/taiyaku-read/src/format.rs`, `crates/taiyaku-read/src/format/tests.rs`
- Modify: `crates/taiyaku-read/src/lib.rs`, `crates/taiyaku-read/src/pages.rs`

**Interfaces:**
- Produces: `taiyaku_read::Format` — `Dir | Zip | EpubFixed | EpubReflow | Pdf | Rar | SevenZip | Tar | Unknown`
- Produces: `taiyaku_read::Format::token(self) -> Option<&'static str>` — the `scan_entry.format` value, `None` for `Unknown`
- Produces: `taiyaku_read::sniff(path: &Path) -> Result<Format, String>`

- [ ] **Step 1: Write the failing tests**

`crates/taiyaku-read/src/format/tests.rs`:

```rust
use super::*;

/// A zip whose entries are `names`, each holding `bytes`.
fn zip_with(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let file = std::fs::File::create(&path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    for (entry, bytes) in entries {
        zip.start_file(*entry, zip::write::SimpleFileOptions::default())
            .expect("start");
        std::io::Write::write_all(&mut zip, bytes).expect("write");
    }
    zip.finish().expect("finish");
    path
}

#[test]
fn a_plain_zip_is_a_zip_whatever_it_is_called() {
    let dir = tempdir();
    // Named .cbr, and README records that this happens constantly.
    let path = zip_with(dir.path(), "volume.cbr", &[("001.jpg", b"x")]);
    assert_eq!(sniff(&path), Ok(Format::Zip));
    assert_eq!(Format::Zip.token(), Some("zip"));
}

#[test]
fn a_directory_is_a_dir() {
    let dir = tempdir();
    assert_eq!(sniff(dir.path()), Ok(Format::Dir));
}

#[test]
fn a_rar_named_cbz_is_a_rar() {
    let dir = tempdir();
    let path = dir.path().join("volume.cbz");
    std::fs::write(&path, b"Rar!\x1a\x07\x00rest").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Rar));
}

#[test]
fn a_pdf_is_a_pdf() {
    let dir = tempdir();
    let path = dir.path().join("book.pdf");
    std::fs::write(&path, b"%PDF-1.7\nrest").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Pdf));
}

#[test]
fn an_epub_is_split_by_its_declared_layout() {
    let dir = tempdir();
    let reflow = zip_with(
        dir.path(),
        "novel.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            ("content.opf", b"<package><metadata></metadata></package>"),
        ],
    );
    assert_eq!(sniff(&reflow), Ok(Format::EpubReflow));

    let fixed = zip_with(
        dir.path(),
        "comic.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            (
                "content.opf",
                b"<package><metadata><meta property=\"rendition:layout\">pre-paginated</meta></metadata></package>",
            ),
        ],
    );
    assert_eq!(sniff(&fixed), Ok(Format::EpubFixed));
}

#[test]
fn a_file_that_is_none_of_them_is_unknown_rather_than_an_error() {
    let dir = tempdir();
    let path = dir.path().join("cover.txt");
    std::fs::write(&path, b"not a container").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Unknown));
    assert_eq!(Format::Unknown.token(), None);
}

#[test]
fn a_path_that_is_not_there_is_an_error_naming_it() {
    let dir = tempdir();
    let path = dir.path().join("gone.cbz");
    let message = sniff(&path).expect_err("a missing path is an error");
    assert!(message.contains("gone.cbz"), "{message}");
}
```

**There is no `tempfile` dependency and you must not add one.** `taiyaku-library/src/tests.rs:3` records the decision in as many words: "this workspace's dependency list is short enough to quote in `docs/status.md`, and a `tempfile` dependency would earn its place for about fifteen lines." `taiyaku-read` has no helper of its own — `atomic/tests.rs:22` inlines `std::env::temp_dir()` — so write one here, in the shape `taiyaku-library` already uses, and give it a `Drop`:

```rust
struct TempDir {
    path: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl TempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

fn tempdir() -> TempDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "taiyaku-test-format-{}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("creating the test directory");
    TempDir { path }
}
```

A `path()` method rather than a bare field, because the tests above call it that way. `taiyaku-library`'s own copy exposes the field instead (`crate::tests::TempDir { path }`), so the two read slightly differently — that is a fifteen-line helper in two crates that cannot share one, not a thing to unify.

`zip::ZipWriter` is available here because `zip` is a real dependency of `taiyaku-read`. It is **not** available in `taiyaku-library`, which is why Task 5's fixtures are directories rather than archives.

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-read format::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write `format.rs`**

```rust
//! What a container is, from its bytes rather than from its name.
//!
//! **Extensions lie in both directions**, which `README.md` records as the
//! reason `taiyaku-read` never trusted one: a zip is frequently named `.cbr`
//! and a real RAR frequently `.cbz`. The scanner's §3 makes that a rule —
//! `scan_entry.format` is sniffed, never derived from a suffix.
//!
//! **Recognising a format is not a step towards reading it.** `Pdf`, `Rar`,
//! `SevenZip` and both EPUB variants exist so a scan can record what it met
//! and a reader can be told why nothing opens it. Only `Zip` and `Dir` are
//! readable, and `pages.rs` is where that stays decided.

use std::io::Read;
use std::path::Path;

/// What a path holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dir,
    Zip,
    EpubFixed,
    EpubReflow,
    Pdf,
    Rar,
    SevenZip,
    Tar,
    /// A file none of the signatures matched. Recorded rather than skipped,
    /// per §3: a scanner that silently drops what it cannot open is the
    /// failure `inventory` was written against.
    Unknown,
}

// **`Tar` is one more variant than §3's list**, which names
// `zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z | NULL`. It is here
// because `pages.rs` already recognises tar and already names it in an error
// — dropping it would mean this enum recorded a tar as `Unknown` while the
// error message beside it called it a tar archive, which is two answers to
// one question. `003_scanner.sql`'s `format` comment carries the same eight
// values for the same reason.

impl Format {
    /// The `scan_entry.format` value, or `None` for a file nothing recognised.
    ///
    /// The strings are the schema's own vocabulary and are not free text; a
    /// change here is a migration, not a rename.
    #[must_use]
    pub fn token(self) -> Option<&'static str> {
        match self {
            Self::Dir => Some("dir"),
            Self::Zip => Some("zip"),
            Self::EpubFixed => Some("epub-fixed"),
            Self::EpubReflow => Some("epub-reflow"),
            Self::Pdf => Some("pdf"),
            Self::Rar => Some("rar"),
            Self::SevenZip => Some("7z"),
            Self::Tar => Some("tar"),
            Self::Unknown => None,
        }
    }

    /// Whether `taiyaku-read` can turn this into pages.
    #[must_use]
    pub fn is_readable(self) -> bool {
        matches!(self, Self::Dir | Self::Zip)
    }
}

const SEVEN_ZIP: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
const RAR: &[u8] = b"Rar!\x1a\x07";
const ZIP: &[u8] = b"PK\x03\x04";
const PDF: &[u8] = b"%PDF-";

/// What is at `path`.
///
/// # Errors
///
/// Returns an error naming the path if it cannot be opened or read. A file
/// that is readable and matches nothing is [`Format::Unknown`], which is a
/// successful answer rather than an error.
pub fn sniff(path: &Path) -> Result<Format, String> {
    if path.is_dir() {
        return Ok(Format::Dir);
    }
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut header = [0u8; 512];
    let read = file
        .read(&mut header)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(match from_header(&header[..read]) {
        // The only branch that needs more than the header, and so the only
        // one that costs a second open. See `zip_flavour`.
        Format::Zip => zip_flavour(path),
        other => other,
    })
}

/// The decision every branch but one can make from the leading bytes alone.
///
/// **The EPUB refinement is deliberately not here**, and that is a
/// performance constraint rather than a tidiness preference. `pages.rs`'s
/// `unsupported_container` calls this on a header it already holds, from
/// inside `open_archive` — which runs on every `member_page`, every
/// `member_pages` and every `member_digest`, so once per page served. A
/// version of this function that opened the archive to look for a `mimetype`
/// entry would double the open count on that path to answer a question the
/// error message does not need: an EPUB handed to `taiyaku align` is a zip
/// holding no pages, and `read_member` already names what it found instead.
/// So this stays header-only and total, and [`sniff`] — which runs once per
/// file per scan, not once per page — does the refinement.
fn from_header(bytes: &[u8]) -> Format {
    if bytes.starts_with(SEVEN_ZIP) {
        Format::SevenZip
    } else if bytes.starts_with(RAR) {
        Format::Rar
    } else if bytes.starts_with(PDF) {
        Format::Pdf
    } else if bytes.get(257..262) == Some(&b"ustar"[..]) {
        Format::Tar
    } else if bytes.starts_with(ZIP) {
        Format::Zip
    } else {
        Format::Unknown
    }
}

/// A zip is a zip unless it declares itself an EPUB, in which case the
/// question is which kind.
///
/// **What this establishes and what it does not.** An EPUB declares its
/// layout in the OPF package document, as
/// `<meta property="rendition:layout">pre-paginated</meta>`. This looks for
/// that string in every `.opf` entry rather than parsing XML — the workspace
/// has no XML dependency and adding one to answer a question nothing yet
/// reads would be the wrong trade. So it is a substring test: an OPF that
/// mentioned `pre-paginated` in a comment or a title would be misread as
/// fixed-layout. Nothing opens either variant (§10), so the cost of being
/// wrong here today is one wrong token in a column no reader consumes; when
/// spec H makes fixed-layout EPUB a real member format, this is the function
/// that needs a parser, and this paragraph is the note saying so.
///
/// A zip whose central directory will not open — a truncated download — is
/// [`Format::Zip`] on the strength of its signature. The scan records what
/// the bytes say; whether the archive is *sound* is a page-count question
/// and `member_pages` answers it separately.
fn zip_flavour(path: &Path) -> Format {
    let Ok(file) = std::fs::File::open(path) else {
        return Format::Zip;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return Format::Zip;
    };
    let mut mimetype = String::new();
    if let Ok(mut entry) = zip.by_name("mimetype") {
        let _ = entry.read_to_string(&mut mimetype);
    }
    if mimetype.trim() != "application/epub+zip" {
        return Format::Zip;
    }
    let opf_names: Vec<String> = zip
        .file_names()
        .filter(|name| name.ends_with(".opf"))
        .map(ToString::to_string)
        .collect();
    for name in opf_names {
        let mut body = String::new();
        if let Ok(mut entry) = zip.by_name(&name) {
            let _ = entry.read_to_string(&mut body);
        }
        if body.contains("pre-paginated") {
            return Format::EpubFixed;
        }
    }
    Format::EpubReflow
}

#[cfg(test)]
mod tests;
```

In `crates/taiyaku-read/src/lib.rs`:

```rust
mod format;
pub use format::{Format, sniff};
```

- [ ] **Step 4: Rewrite `unsupported_container` on top of it**

This is what stops there being two sniffers. In `crates/taiyaku-read/src/pages.rs`, replace the body of `unsupported_container` — keep the function, its name and its callers — with a match over `crate::format::from_header`:

```rust
/// Name a container taiyaku cannot read, from its leading bytes.
///
/// Only zip-based archives and directories are supported. Recognising the
/// others is not a step towards reading them — it is so the error can say
/// which format it found rather than reporting a corrupt zip, since the
/// extension is frequently wrong about what is actually inside.
///
/// **The recognition itself lives in `format.rs` since 2026-08-17**, because
/// the scanner needs the same answer as a token rather than as prose and two
/// tables of magic numbers would drift. This is the prose half; that is the
/// decision half.
///
/// An EPUB is **not** named here, deliberately. Telling one from a plain zip
/// means opening the archive and reading its `mimetype` entry, and this
/// function runs inside `open_archive` — once per page served. `read_member`
/// already reports what a page-less archive held instead, which is the
/// diagnosis that actually helps.
fn unsupported_container(bytes: &[u8]) -> Option<&'static str> {
    match crate::format::from_header(bytes) {
        crate::Format::SevenZip => Some("a 7-Zip archive"),
        crate::Format::Rar => Some("a RAR archive"),
        crate::Format::Tar => Some("a tar archive"),
        crate::Format::Pdf => Some("a PDF"),
        _ => None,
    }
}
```

`from_header` becomes `pub(crate)`. **Its signature does not change** — header bytes only — so `open_archive`'s call site is untouched.

*An earlier draft of this task had `from_header` take a `&Path` and do the EPUB refinement itself, with `unsupported_container` reporting `"an EPUB"`. That is a **performance defect**, caught before the task was dispatched and recorded here rather than deleted: `unsupported_container` is called from `open_archive`, which runs on every `member_page`, `member_pages` and `member_digest` — once per page served — so refining there would have opened every archive a second time on the hot path, to improve one error message. The refinement belongs in `sniff`, which runs once per file per scan.*

**One behaviour changes:** a PDF handed to `taiyaku align` used to be reported as an unreadable zip and is now named, because `from_header` recognises `%PDF-` and the old function did not. The existing tests asserting the RAR, 7-Zip and tar wordings must not change.

- [ ] **Step 4b: Cover the new arm where this codebase already covers the others**

`crates/taiyaku-read/src/pages/tests.rs`'s `an_unreadable_container_is_named_by_its_leading_bytes` asserts **every** branch of `unsupported_container` — 7-Zip, both RAR generations, tar, and zip-is-fine-so-`None`. A new arm that is not in it breaks that convention, and `format/tests.rs`'s `a_pdf_is_a_pdf` does not close the gap: it asserts `sniff` returns `Format::Pdf`, which never reaches the string mapping at all. A typo in the literal — `"A PDF"`, or a dropped word — would pass every test in this task.

Add the case to the existing test rather than writing a new one:

```rust
    let pdf = b"%PDF-1.7\n";
```

and, beside the others:

```rust
    assert_eq!(unsupported_container(pdf), Some("a PDF"));
```

Then mirror `reading_a_seven_zip_archive_says_so`, which is how this file tests the same fact end to end through `read_member`, so the wording is pinned on the path a person actually meets:

```rust
/// A PDF is named rather than reported as a corrupt zip, the same way a
/// 7-Zip archive is. New with the scanner's sniffer: `unsupported_container`
/// recognised three formats before it and recognises four now.
#[test]
fn reading_a_pdf_says_so() {
    // Shape copied from `reading_a_seven_zip_archive_says_so` directly
    // above: a bare file under `std::env::temp_dir()`, no subdirectory, and
    // `read_member` rather than `member_pages` — that is the call which
    // produces the message a person actually meets.
    let path = std::env::temp_dir().join("taiyaku-test-read-pdf.pdf");
    std::fs::write(&path, b"%PDF-1.7\n").expect("write");

    let message = read_member(&path).expect_err("a PDF is not readable");

    assert!(message.contains("this is a PDF"), "{message}");
}
```

**Read `reading_a_seven_zip_archive_says_so` and follow its exact shape.** If it differs from the sketch above, it wins and the sketch is stale — say so in your report. *An earlier version of this step was internally inconsistent, its prose naming `read_member` while its code called `member_pages`; the implementer caught that and followed the neighbouring test, which is the correct resolution and is why the instruction is phrased this way.*

- [ ] **Step 5: Run the tests**

Run: `cargo test -p taiyaku-read`
Expected: PASS, including `pages`' existing tests for the RAR, 7-Zip and tar messages — those must not change wording.

- [ ] **Step 6: Gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "One sniffer, answering both in prose and in the scanner's vocabulary"
```

---

## Task 3: `import` gains a path-less entry point

Correction 4. A scanned work has no `taiyaku.json`, so `insert_work`'s unconditional `sidecar_path` has nothing honest to write. The alternative — the scanner inserting `work`, `edition` and `member` rows itself — is a second writer of the three tables the store spec's §4.2 spends its length keeping to one, and it would need its own copy of `unique_slug`, `content_key` and the replace-wholesale transaction.

**Files:**
- Modify: `crates/taiyaku-library/src/import.rs`
- Test: `crates/taiyaku-library/src/import/tests.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks
- Produces: `taiyaku_library::import::scanned(conn: &Connection, doc: &Sidecar) -> Result<i64, String>`

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/import/tests.rs`. **Use the fixture builders already in that file — do not add new ones.** Checked on 2026-08-17, they are:

- `doc(hub_sha: &str)` (`import/tests.rs:5`) — one edition, key `raw`, title `T`, one member whose `sha256` is the argument.
- `two_edition_doc()` (`import/tests.rs:20`) — hub `raw` with member sha `hh`, plus a spoke `en` and an alignment. Its own doc comment records that it deliberately shares its hub member's `sha256`, and so its `content_key`, with `hub_only_doc`.

That second fact is what makes the third test below work without a new helper: `doc("hh")` and `two_edition_doc()` have the same `content_key` by construction.

The module is `mod tests` with `use super::*`, so the functions under test are called bare — `sidecar(&conn, …)`, `scanned(&conn, …)` — exactly as `importing_the_same_document_twice_is_one_work` (`import/tests.rs:451`) already does.

```rust
#[test]
fn a_scanned_work_is_stored_with_no_sidecar_path() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let id = scanned(&conn, &doc("aa")).expect("import");

    let path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("select");
    assert_eq!(path, None, "a scanned work claims no file on disk");
}

#[test]
fn two_scanned_works_in_one_directory_do_not_collide() {
    // The case that made this function necessary: a series directory holds
    // fourteen volumes as files, so `sidecar_path` could only ever have named
    // the directory and all fourteen would have matched each other on
    // `find_by_path`. Identity is `content_key` alone here — which is why
    // these two, sharing a title and differing only in their member digest,
    // are two works.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let first = scanned(&conn, &doc("aa")).expect("first");
    let second = scanned(&conn, &doc("bb")).expect("second");

    assert_ne!(first, second);
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2);
}

#[test]
fn aligning_a_scanned_work_later_lands_on_the_same_row() {
    // The whole point: the scanner records a volume, someone aligns it, and
    // `align --write` reaches the same work rather than minting a second —
    // so the scan's series, volume and any reading position come with it.
    // `two_edition_doc`'s hub member sha is `hh`, matching `doc("hh")`, and
    // `content_key` hashes the hub edition's members alone.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let scanned_id = scanned(&conn, &doc("hh")).expect("scan");

    let aligned_id = sidecar(
        &conn,
        &two_edition_doc(),
        Path::new("/library/series/taiyaku.json"),
    )
    .expect("align");

    assert_eq!(scanned_id, aligned_id, "the same volume, not a second work");
    let path: Option<String> = conn
        .query_row(
            "SELECT sidecar_path FROM work WHERE id = ?1",
            [aligned_id],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(path.as_deref(), Some("/library/series/taiyaku.json"));
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 1);
}
```

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library import::tests::a_scanned_work`
Expected: FAIL — `no function or associated item named 'scanned'`.

- [ ] **Step 3: Thread `Option<&Path>` through the private helpers**

In `crates/taiyaku-library/src/import.rs`, `sidecar` keeps its signature — every existing caller compiles unchanged — and both entry points delegate to one body:

```rust
/// Import one sidecar, returning the `work.id` it landed on.
///
/// # Errors
///
/// Returns an error if the document declares a format version newer than this
/// build reads (see [`readable_version`]), if `content_key` fails — a hub
/// naming no edition — or if SQLite refuses a statement.
pub fn sidecar(conn: &Connection, doc: &Sidecar, path: &Path) -> Result<i64, String> {
    import_with(conn, doc, Some(path))
}

/// Import a work the **scanner** derived, which has no document on disk.
///
/// `work.sidecar_path` stays NULL and identity is [`crate::identity::content_key`]
/// alone. There is no honest path to record: the corpus convention is one
/// `taiyaku.json` per work directory (`docs/sidecar.md`), while a scanned
/// series directory holds every volume as a file beside its siblings — so a
/// path-keyed identity would make all fourteen of them the same work.
///
/// **What this buys, and the one case where it does not.** When someone later
/// runs `align --write` over the same archives, that import matches on
/// `content_key` and lands on *this* row, carrying the scan's series, volume
/// and reading position with it. `content_key` is a hash over the **hub**
/// edition's members, so this holds exactly while the alignment's hub is the
/// edition the scan derived — the A side of `taiyaku align a... -- b...`.
/// Align the other way round and the key differs, a second work is inserted,
/// and the scanned row is left behind holding whatever progress it had. That
/// is not fixed here, because fixing it means a second identity probe over
/// *any* member rather than the hub's, which would merge two genuinely
/// different works that happen to share one file. Recorded rather than
/// guessed at: if it turns up in real use, the evidence is a duplicate pair
/// of works whose editions do not overlap.
///
/// # Errors
///
/// The same as [`sidecar`].
pub fn scanned(conn: &Connection, doc: &Sidecar) -> Result<i64, String> {
    import_with(conn, doc, None)
}

fn import_with(conn: &Connection, doc: &Sidecar, path: Option<&Path>) -> Result<i64, String> {
    readable_version(doc)?;
    let key = crate::identity::content_key(doc)?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting import transaction: {e}"))?;

    let existing = match find_by_key(&tx, &key)? {
        Some(id) => Some(id),
        // No path, no second probe. A scanned work is its content and
        // nothing else.
        None => match path {
            Some(path) => find_by_path(&tx, path)?,
            None => None,
        },
    };

    let work_id = match existing {
        Some(id) => {
            update_work(&tx, id, doc, path, &key)?;
            id
        }
        None => insert_work(&tx, doc, path, &key)?,
    };

    // Only meaningful when a path was claimed: content decided `work_id`, so
    // any other row still pointing at that path is now lying about disk.
    if let Some(path) = path {
        clear_stale_path_claim(&tx, work_id, path)?;
    }

    replace_editions(&tx, work_id, doc)?;
    replace_alignments(&tx, work_id, doc)?;

    tx.commit()
        .map_err(|e| format!("committing import of work {work_id}: {e}"))?;
    Ok(work_id)
}
```

`insert_work` and `update_work` take `Option<&Path>` and bind `path.map(|p| p.to_string_lossy().into_owned())`, which is `NULL` for `None`.

**`update_work` must not null an existing path when handed `None`.** A rescan of a work that has since been aligned would otherwise erase the pointer to its `taiyaku.json`. Its `SET` clause becomes conditional:

The whole statement, with the parameter numbering `update_work` already uses:

```rust
// COALESCE, not an unconditional assignment: a rescan arrives with no path
// and must leave a path an earlier `align --write` established. The reverse
// — a real path replacing NULL — is the ordinary assignment and still
// happens, since COALESCE takes the non-NULL argument.
conn.execute(
    "UPDATE work SET content_key = ?1, title = ?2, reading = ?3, hub = ?4,
            sidecar_path = COALESCE(?5, sidecar_path)
     WHERE id = ?6",
    rusqlite::params![
        key,
        doc.work.title,
        direction_text(doc.work.reading),
        doc.hub,
        path.map(|p| p.to_string_lossy().into_owned()),
        work_id,
    ],
)
.map_err(|e| format!("updating work {work_id}: {e}"))?;
```

`rusqlite` binds `Option<String>` as SQL NULL for `None`, which is what makes `COALESCE` see the missing path rather than an empty string.

**`update_work` does not touch `sidecar_mtime`** — checked against `import.rs:337-358` on 2026-08-17, where the `SET` clause is exactly the five columns above. `sidecar_path` is the only one needing this treatment.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p taiyaku-library import::`
Expected: PASS, all three new tests and every existing import test unchanged.

- [ ] **Step 5: Add the test that pins the COALESCE**

```rust
#[test]
fn rescanning_an_aligned_work_does_not_erase_its_sidecar_path() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = sidecar(&conn, &doc("aa"), Path::new("/library/series/taiyaku.json")).expect("align");

    scanned(&conn, &doc("aa")).expect("rescan");

    let path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("select");
    assert_eq!(path.as_deref(), Some("/library/series/taiyaku.json"));
}
```

Run: `cargo test -p taiyaku-library import::tests::rescanning`
Expected: PASS.

- [ ] **Step 6: Gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "A work the scanner derived has no document on disk, and import can now say so"
```

---

## Task 4: Migration 3 — `scan_entry`, `dir_state`, `rule`, `proposal`, and four seeded rules

§3, §4 and §6's schemas, plus the defaults §4 says are "seeded as ordinary rules, visible and editable, with nothing hidden in code". `root` is already in migration 1 and is not touched.

**One value extends the spec's list, deliberately.** §3 names `format` as `zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z | NULL`; the column comment below carries `tar` as well, matching Task 2's `Format` enum. `pages.rs` already recognises a tar and already names it in an error message, so leaving it out would have this column record `NULL` for a file the error beside it calls a tar archive — two answers to one question.

**Files:**
- Create: `crates/taiyaku-library/src/schema/003_scanner.sql`
- Modify: `crates/taiyaku-library/src/schema.rs`
- Test: `crates/taiyaku-library/src/schema/tests.rs`

**Interfaces:**
- Produces: `SCHEMA_VERSION == 3`, and the four tables named above

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-library/src/schema/tests.rs`, **following that file's own conventions**, checked on 2026-08-17: its tests open a bare `Connection::open_in_memory()` and call `migrate(&conn)` directly rather than going through `Library`, and it already has a `table_names(conn) -> Vec<String>` helper (`schema/tests.rs:230`) that `migration_one_creates_every_table_the_spec_names` (`:241`) uses. Reuse both; do not open a `Library` here and do not write a second table-listing query.

The one place you must open a `Library` instead is any test needing the `foreign_keys` pragma — `migrate` alone does not set it. `two_members_cannot_share_an_ordinal_within_an_edition` (`:285`) sets it by hand with `conn.pragma_update(None, "foreign_keys", "ON")`; the second test below needs the same line, because it inserts a `proposal` row referencing `series`.

```rust
#[test]
fn migration_three_creates_the_scanner_tables_and_seeds_the_default_rules() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migration");

    let names = table_names(&conn);
    for expected in ["dir_state", "proposal", "rule", "scan_entry"] {
        assert!(
            names.contains(&expected.to_string()),
            "missing table `{expected}`: {names:?}"
        );
    }

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rules, 4, "§4's four defaults, seeded as ordinary rows");

    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM rule ORDER BY ordinal")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        kinds,
        ["regex", "builtin-volume", "builtin-chapter", "regex"]
    );
}

#[test]
fn a_dismissed_proposal_is_not_raised_again_when_its_target_is_null() {
    // SQLite treats NULLs as distinct in a UNIQUE constraint, so a plain
    // `UNIQUE (kind, subject_id, target_id)` would let every rescan insert a
    // second `attach-chapters` row for the same directory — and §6's whole
    // argument is that dismissal has to persist or the inbox becomes noise.
    // The expression index is what closes that.
    let conn = Connection::open_in_memory().expect("in-memory connection");
    // `migrate` does not set this and the insert below crosses a foreign key.
    // Same line, for the same reason, as `two_members_cannot_share_an_ordinal
    // _within_an_edition` above.
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys");
    migrate(&conn).expect("migration");
    conn.execute("INSERT INTO series (id, sort_title) VALUES (1, 'Series')", [])
        .expect("series");
    let insert = "INSERT OR IGNORE INTO proposal (kind, subject_id, target_id, score, reason, state)
                  VALUES ('attach-chapters', 1, NULL, 0.5, 'because', ?1)";
    conn.execute(insert, ["dismissed"]).expect("first");
    conn.execute(insert, ["open"]).expect("second");

    let (count, state): (i64, String) = conn
        .query_row(
            "SELECT count(*), max(state) FROM proposal WHERE kind = 'attach-chapters'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select");
    assert_eq!(count, 1);
    assert_eq!(state, "dismissed", "the dismissal survived the rescan");
}
```

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library schema::tests::migration_three`
Expected: FAIL — `no such table: scan_entry`.

- [ ] **Step 3: Write `003_scanner.sql`**

```sql
-- The fact layer -------------------------------------------------------------
--
-- What is on disk, and nothing about what it means. `derive.rs` is the only
-- module that reads these tables for structure; this one is written by a walk
-- that decides nothing (§2).
--
-- Persisted rather than streamed, because that is what buys the property a
-- direct scanner cannot have: editing a rule re-derives the whole library
-- without touching the filesystem. `pages` and `sha256` are on this table for
-- the same reason — a re-derive that had to reopen every archive to recount
-- and rehash would be a rescan wearing a different name.
CREATE TABLE scan_entry (
  id         INTEGER PRIMARY KEY,
  root_id    INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  rel_path   TEXT NOT NULL,
  kind       TEXT NOT NULL,              -- file | dir
  format     TEXT,                       -- zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z | tar
  size       INTEGER NOT NULL,
  mtime      INTEGER NOT NULL,
  pages      INTEGER,
  sha256     TEXT,
  first_seen TEXT NOT NULL DEFAULT (datetime('now')),
  last_seen  TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE (root_id, rel_path)
);

-- Per-directory state, for §8's cheap walk: an interval scan compares these
-- two numbers before doing anything expensive in a directory.
--
-- **It does not license skipping the descent.** A directory's mtime moves when
-- an entry is added, removed or renamed *in it*, and not when a file's content
-- changes or when something changes two levels down — so a walk that declined
-- to recurse into an unchanged directory would miss every deep change. What
-- these numbers save is opening containers, which is the expensive part; see
-- `scan.rs`'s doc comment.
CREATE TABLE dir_state (
  root_id     INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  rel_path    TEXT NOT NULL,
  mtime       INTEGER NOT NULL,
  entry_count INTEGER NOT NULL,
  last_walked TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (root_id, rel_path)
) WITHOUT ROWID;

CREATE INDEX scan_entry_by_root ON scan_entry (root_id);

-- The rule layer -------------------------------------------------------------
--
-- Ordered layering, per field: every matching rule applies in `ordinal` order,
-- and a later rule overrides only the fields it actually sets (§4). So "this
-- whole root is raw" and "except these are the colour edition" are two rules
-- rather than four. First-match-wins was rejected because every exception
-- would have to restate the general case; most-specific-wins was rejected
-- because the engine would then be making a judgement the user cannot see or
-- reorder.
--
-- `root_id` NULL means every root.
--
-- **Which fields a rule sets is not all in the `set_` columns.** A `regex`
-- rule also sets whatever its named captures provide — `series`, `volume`,
-- `chapter`, `edition`, `title` — and `builtin-volume` / `builtin-chapter` set
-- exactly one field each, from `taiyaku_read::volume_number` and
-- `chapter_number`. The `set_` columns are the literal overrides, which is why
-- there is no `set_volume`: a literal volume number is a property of one file,
-- not of a rule that matches many.
CREATE TABLE rule (
  id              INTEGER PRIMARY KEY,
  ordinal         INTEGER NOT NULL,
  enabled         INTEGER NOT NULL DEFAULT 1,
  root_id         INTEGER REFERENCES root(id) ON DELETE CASCADE,
  kind            TEXT NOT NULL,          -- prefix | regex | builtin-volume | builtin-chapter
  pattern         TEXT NOT NULL,
  match_on        TEXT NOT NULL,          -- path | name | parent
  set_edition_key TEXT,
  set_kind        TEXT,
  set_language    TEXT,
  set_series      TEXT,
  note            TEXT
);

-- Proposals ------------------------------------------------------------------
--
-- The judgements §1's measurement says no rule should make. Suffix-stripping
-- as an edition signal scored roughly 20 right to 17 wrong on a hand
-- classification of the real library, which is barely better than a coin flip,
-- so this table proposes and nothing merges.
--
-- `subject_id` keys on the SERIES ROW rather than on anything content-derived.
-- A dismissed proposal keyed on a content hash returns as a brand-new proposal
-- the moment a volume is added, and the dismissal silently stops meaning
-- anything. This is also why `derive.rs` reconciles series by `sort_title`
-- instead of deleting and re-inserting them: re-minting a series id would
-- orphan every dismissal that pointed at it, which is the same defect arriving
-- by a different road.
CREATE TABLE proposal (
  id              INTEGER PRIMARY KEY,
  kind            TEXT NOT NULL,          -- edition-of | same-series | attach-chapters | align
  subject_id      INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  target_id       INTEGER REFERENCES series(id) ON DELETE CASCADE,
  score           REAL NOT NULL,
  reason          TEXT NOT NULL,
  state           TEXT NOT NULL DEFAULT 'open',   -- open | accepted | dismissed
  created         TEXT NOT NULL DEFAULT (datetime('now')),
  -- §6.1: accepting a proposal writes a narrow `rule` rather than writing
  -- structure directly, so derivation keeps exactly one input. This column is
  -- what makes that reversible — un-accepting deletes the rule and reopens the
  -- proposal. ON DELETE SET NULL rather than CASCADE: the proposal outlives
  -- its rule, and `proposals::reconcile` reopens any accepted row whose rule
  -- has gone, so a rule deleted through C4's editor cannot leave a proposal
  -- claiming an acceptance that no longer decides anything.
  created_rule_id INTEGER REFERENCES rule(id) ON DELETE SET NULL
);

-- **An expression index, not a plain UNIQUE.** SQLite treats NULLs as distinct
-- in a UNIQUE constraint, so `UNIQUE (kind, subject_id, target_id)` would let
-- every rescan insert a second `attach-chapters` row — target NULL — for a
-- directory somebody already dismissed. §6's argument is that dismissal has to
-- persist or the inbox becomes noise and then becomes ignored, and an
-- INSERT OR IGNORE is only idempotent against a constraint that actually
-- matches.
CREATE UNIQUE INDEX proposal_identity
  ON proposal (kind, subject_id, ifnull(target_id, -1));

-- The defaults, as ordinary editable rows (§4) -------------------------------
--
-- Series defaults to the PARENT DIRECTORY rather than to a filename capture,
-- because a filename does not always repeat the series and a directory always
-- names it. Rule 4 layers a filename capture over it where one exists, which
-- is the layering model doing its job on its own defaults.
INSERT INTO rule (ordinal, kind, pattern, match_on, note) VALUES
  (1, 'regex',           '^(?<series>[^/]+)/', 'path', 'series from the first directory under the root'),
  (2, 'builtin-volume',  '',                   'name', 'volume from the name, via taiyaku_read::volume_number'),
  (3, 'builtin-chapter', '',                   'name', 'chapter from the name, via taiyaku_read::chapter_number'),
  (4, 'regex',           '^(?<series>.+?)[ _-]+v', 'name', 'series from the filename where it repeats it');
```

- [ ] **Step 4: Register it**

In `crates/taiyaku-library/src/schema.rs`:

```rust
pub const SCHEMA_VERSION: u32 = 3;

const MIGRATIONS: [&str; 3] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
];
```

The `const` assertion below them already checks the two agree; nothing else in the module changes.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p taiyaku-library schema::`
Expected: PASS, both new tests and every existing migration test — including the one that migrates an existing version-2 file forward.

- [ ] **Step 6: Gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "Migration 3: facts, rules and proposals, with the defaults seeded as rows anyone can edit"
```

---

## Task 5: The walk and the fact layer

§3. Rows about what is on disk, and no decisions at all. This task ends with a scan that fills `scan_entry` and `dir_state` for a root and reports what it found; nothing yet reads those rows.

**Files:**
- Create: `crates/taiyaku-library/src/scan.rs`, `crates/taiyaku-library/src/scan/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs`

**Interfaces:**
- Consumes: `taiyaku_read::{Format, sniff, member_pages}` (Task 2)
- Produces:
  - `taiyaku_library::scan::Root { id: i64, path: PathBuf, edition_key: Option<String>, default_kind: Option<String>, default_language: Option<String>, scan_interval_secs: Option<i64> }`
  - `scan::roots(conn: &Connection) -> Result<Vec<Root>, String>`
  - `scan::add_root(conn: &Connection, path: &Path) -> Result<Root, String>`
  - `scan::ScanReport { root: PathBuf, dirs: usize, files: usize, added: usize, updated: usize, gone: usize, refused: Option<String> }`
  - `scan::scan_root(conn: &Connection, root: &Root) -> Result<ScanReport, String>`

- [ ] **Step 1: Write the failing tests**

`crates/taiyaku-library/src/scan/tests.rs`:

```rust
use super::*;

/// A root holding two series directories, one of them chapter-organised.
///
/// **Members are directories of page images, not `.cbz` files**, and that is
/// forced rather than chosen: `taiyaku-library` does not depend on `zip` and
/// must not start — its dependency list is five entries and `docs/status.md`
/// quotes it. `taiyaku_read::member_pages` reads a directory member exactly as
/// it reads an archive (`pages.rs`'s `directory_page_paths`), so the fact
/// layer is exercised the same way. Archive sniffing is Task 2's to test, in
/// the crate that has the zip writer.
fn fixture() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("scan");
    let root = &dir.path;
    let series = root.join("Go-Toubun no Hanayome");
    write_pages(&series.join("Go-Toubun no Hanayome - v01 (2019)"), 3);
    write_pages(&series.join("Go-Toubun no Hanayome - v02 (2019)"), 4);
    let chapters = root.join("Kaguya-sama - Fan Translated");
    write_pages(&chapters.join("Kaguya-sama - c001"), 2);
    // Recorded, not skipped (§3): a format nothing can read. Five bytes of
    // header is all `sniff` looks at.
    std::fs::write(root.join("notes.pdf"), b"%PDF-1.7\n").expect("write");
    dir
}

/// A directory member holding `pages` files `is_page_name` accepts.
///
/// **Each page's bytes name the directory that holds it**, so two members are
/// never byte-identical by accident. The first version wrote the same
/// placeholder into every page, which meant any two volumes with equal page
/// counts hashed to the same member digest, produced the same `content_key`,
/// and **silently merged into one `work` row** — `derive`'s documented
/// duplicated-download case, reached by a fixture rather than by a corpus.
/// It went unnoticed until a Task 8 fixture happened to give two volumes the
/// same length, and then surfaced as a proposal count that was short by one.
///
/// Varying the counts per volume also avoids it, and that is what the fixtures
/// did first — but a helper that is only correct while every caller remembers
/// to vary something is not correct, it is lucky. `lib.rs` makes the same
/// argument about `recursive_triggers`, in the same words.
///
/// A test that *wants* two identical members writes them itself; deliberate
/// duplication should be visible at the call site, not inherited from a
/// helper's placeholder.
fn write_pages(dir: &Path, pages: usize) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let label = dir.to_string_lossy().into_owned();
    for page in 0..pages {
        std::fs::write(
            dir.join(format!("{page:03}.jpg")),
            format!("not really a jpeg: {label} page {page}").as_bytes(),
        )
        .expect("write");
    }
}

#[test]
fn a_scan_records_every_entry_with_the_format_its_bytes_say() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.files, 1);
    assert_eq!(report.dirs, 6);
    assert_eq!(report.added, 7);
    assert_eq!(report.refused, None);

    let formats: Vec<(String, Option<String>, Option<i64>)> = conn
        .prepare("SELECT rel_path, format, pages FROM scan_entry WHERE kind = 'file' ORDER BY rel_path")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");

    assert!(
        formats
            .iter()
            .any(|(p, f, pages)| p == "notes.pdf" && f.as_deref() == Some("pdf") && pages.is_none()),
        "a PDF is recorded with its format and no page count, never skipped"
    );

    let dirs: Vec<(String, Option<String>, Option<i64>)> = conn
        .prepare("SELECT rel_path, format, pages FROM scan_entry WHERE kind = 'dir' ORDER BY rel_path")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");

    assert!(
        dirs.iter().any(|(p, f, pages)| p.ends_with("v01 (2019)")
            && f.as_deref() == Some("dir")
            && *pages == Some(3)),
        "{dirs:?}"
    );
    assert!(
        dirs.iter().any(|(p, _, pages)| p == "Go-Toubun no Hanayome" && *pages == Some(7)),
        "a container directory carries its recursive total, which is what lets \
         derivation pick the deepest directory that is actually a member"
    );
}

#[test]
fn the_page_images_inside_a_directory_member_are_counted_and_not_recorded() {
    // Nine page files exist on disk under this fixture. Recording one row per
    // page would put hundreds of thousands of rows in `scan_entry` for a real
    // library — 156 series of directory members — to hold facts that
    // derivation never reads one at a time. `is_page_name` is the test, and it
    // is the same function `read_edition` uses, so what counts as a page is
    // decided in one place.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.files, 1, "notes.pdf, and none of the nine page images");
    assert_eq!(report.dirs, 6);
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM scan_entry", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 7);
}

#[test]
fn a_second_scan_of_an_unchanged_tree_re_reads_nothing() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");
    scan_root(&conn, &root).expect("first");

    let report = scan_root(&conn, &root).expect("second");

    assert_eq!(report.added, 0);
    assert_eq!(report.updated, 0, "nothing moved, so nothing was reopened");
    assert_eq!(report.files, 1, "still seen, just not re-read");
}

#[test]
fn a_file_that_has_gone_is_recorded_as_gone_rather_than_left_behind() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");
    scan_root(&conn, &root).expect("first");

    std::fs::remove_file(tree.path.join("notes.pdf")).expect("remove");
    let report = scan_root(&conn, &root).expect("second");

    assert_eq!(report.gone, 1);
    let left: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path = 'notes.pdf'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(left, 0);
}

#[test]
fn a_root_that_has_become_empty_is_refused_and_nothing_is_applied() {
    // §8.1. An unmounted NFS share, a container started before its bind mount
    // was ready and a renamed directory all produce exactly this scan, and
    // applying it would drop every entry, re-derive to no works, and take the
    // reader's progress and notes with it.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");
    scan_root(&conn, &root).expect("first");

    std::fs::remove_dir_all(&tree.path).expect("remove");
    std::fs::create_dir_all(&tree.path).expect("recreate");
    let report = scan_root(&conn, &root).expect("second");

    let refusal = report.refused.expect("an empty root is refused");
    assert!(refusal.contains("empty"), "{refusal}");
    let kept: i64 = conn
        .query_row("SELECT count(*) FROM scan_entry", [], |r| r.get(0))
        .expect("count");
    assert_eq!(kept, 7, "nothing was applied");
}

#[test]
fn a_symlink_cycle_is_an_error_naming_the_depth_rather_than_a_silent_truncation() {
    let tree = crate::tests::tempdir("cycle");
    let deep = tree.path.join("a");
    std::fs::create_dir_all(&deep).expect("mkdir");
    std::os::unix::fs::symlink(&tree.path, deep.join("loop")).expect("symlink");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");

    let message = scan_root(&conn, &root).expect_err("a cycle is an error");
    assert!(message.contains("32"), "{message}");
}
```

`crate::tests::tempdir` and `crate::tests::TempDir` already exist (`taiyaku-library/src/tests.rs:11-31`) and are `pub(crate)` for exactly this reason. Do not write a fourth copy and do not reach for `tempfile`.

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library scan::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write `scan.rs`**

The module doc first, because it carries the decision a reader will otherwise get wrong:

```rust
//! The walk: what is on disk, recorded, with nothing decided about it.
//!
//! §2 of the scanner spec — a scan produces facts, and rules turn facts into
//! structure. Nothing in this module knows what a series is. `derive.rs` is
//! the module that reads these rows and decides, and it never touches the
//! filesystem.
//!
//! **Two passes, borrowed from `inventory::walk`'s shape rather than its
//! policy.** The first measures — every directory's recursive page total — and
//! the second records. `inventory` needs two passes because it must know
//! whether a *child* qualifies before deciding a parent's fate; this module
//! needs them because a directory-format edition's page count is the sum of
//! what is beneath it, and a single pass would have to record a row before it
//! could know that number.
//!
//! **What the cheap walk actually saves, since the phrase invites the wrong
//! implementation.** `dir_state`'s mtime and entry count do NOT license
//! skipping a descent: a directory's mtime moves when an entry is added,
//! removed or renamed *in it*, and not when a file's content changes or when
//! anything at all happens two levels down. A walk that declined to recurse
//! into an unchanged directory would miss every deep change, silently, which
//! is the worst failure this module could have. What is skipped is **opening
//! containers** — `member_pages` lists a zip's entries and is the expensive
//! call here — and an entry is only reopened when its `(size, mtime)` has
//! moved from what `scan_entry` already holds. Hashing is skipped entirely:
//! `sha256` stays NULL until `derive.rs` needs it, per §3.
//!
//! **This module writes nothing to the library tree.** §10. The one file the
//! scanner ever creates is `watch.rs`'s probe, which is not this.
```

Then the shape. `Root` is a plain projection of the `root` row; `roots` and `add_root` are two small queries. `scan_root` is:

```rust
pub fn scan_root(conn: &Connection, root: &Root) -> Result<ScanReport, String> {
    let measured = measure(&root.path, 0)?;
    let seen = flatten(&measured);

    // §8.1, before a single row is written.
    //
    // **The test is "nothing under the root", not "no files".** A first draft
    // of this asked whether every entry was a directory, which is true of a
    // perfectly healthy root whose members are directories of page images —
    // most of this corpus — so it would have refused every scan of one. What
    // an unmounted share actually looks like is an empty directory: the root
    // itself and nothing beneath it.
    if seen.len() <= 1 {
        let held: i64 = conn
            .query_row(
                "SELECT count(*) FROM scan_entry WHERE root_id = ?1",
                [root.id],
                |r| r.get(0),
            )
            .map_err(|e| format!("counting what this root already held: {e}"))?;
        if held > 1 {
            return Ok(ScanReport {
                root: root.path.clone(),
                dirs: seen.len(),
                files: 0,
                added: 0,
                updated: 0,
                gone: 0,
                refused: Some(format!(
                    "{} scanned empty while the library holds {held} entries for it — \
                     refusing to apply. An unmounted share, a bind mount that was not \
                     ready, or a renamed directory all look exactly like this, and \
                     applying it would drop every work under this root along with the \
                     reading positions and notes attached to them. If the root has \
                     genuinely become empty, remove it.",
                    root.path.display()
                )),
            });
        }
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the scan transaction: {e}"))?;
    let mut report = ScanReport::for_root(&root.path);
    for entry in &seen {
        record(&tx, root, entry, &mut report)?;
    }
    report.gone = forget_unseen(&tx, root.id, &seen)?;
    tx.commit()
        .map_err(|e| format!("committing the scan of {}: {e}", root.path.display()))?;
    Ok(report)
}
```

The three pieces that loop refers to, so none of it is left as prose:

```rust
/// One thing the walk found, before anything has been decided about it.
struct Seen {
    rel_path: String,
    kind: Kind,
    /// The `scan_entry.format` token, straight from
    /// [`taiyaku_read::Format::token`].
    ///
    /// **`None` only for a file whose bytes matched nothing.** A directory is
    /// always `Some("dir")` — `record` binds this field into the column
    /// directly, and the column is what `derive.rs` reads to tell a directory
    /// member from a container it cannot open. An earlier version of this
    /// comment said `None` for a directory, which would have made every
    /// directory row indistinguishable from an unrecognised file.
    format: Option<&'static str>,
    size: u64,
    mtime: i64,
    /// Recursive page total for a directory; `None` for a file until
    /// [`record`] decides it needs one.
    pages: Option<u32>,
    /// Directories only: how many entries `read_dir` returned, for
    /// `dir_state`. See the module doc for what this does and does not
    /// license skipping.
    entry_count: Option<i64>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    File,
    Dir,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Dir => "dir",
        }
    }
}
```

```rust
/// Write one entry, opening its container only if it is new or has moved.
///
/// **`(size, mtime)` is the whole cheap-walk saving.** `member_pages` lists a
/// zip's entries or walks a directory, and that is the expensive call here;
/// everything else is a `stat` the walk already did. So an entry whose size
/// and mtime match what `scan_entry` holds keeps its stored `pages` and
/// `sha256` and is only touched to move `last_seen` forward.
///
/// `sha256` stays NULL either way — §3 makes hashing lazy, and `derive.rs` is
/// what fills it in when a member is about to become part of a work.
fn record(
    conn: &Connection,
    root: &Root,
    seen: &Seen,
    report: &mut ScanReport,
) -> Result<(), String> {
    let held: Option<(i64, i64)> = conn
        .query_row(
            "SELECT size, mtime FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, seen.rel_path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("reading what {} held: {e}", seen.rel_path))?;

    let size = i64::try_from(seen.size).unwrap_or(i64::MAX);
    let unchanged = held == Some((size, seen.mtime));

    match (held.is_some(), unchanged) {
        (true, true) => {
            conn.execute(
                "UPDATE scan_entry SET last_seen = datetime('now')
                 WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![root.id, seen.rel_path],
            )
            .map_err(|e| format!("touching {}: {e}", seen.rel_path))?;
        }
        (true, false) => {
            report.updated += 1;
            let pages = pages_of(root, seen);
            conn.execute(
                "UPDATE scan_entry
                    SET kind = ?3, format = ?4, size = ?5, mtime = ?6, pages = ?7,
                        sha256 = NULL, last_seen = datetime('now')
                  WHERE root_id = ?1 AND rel_path = ?2",
                rusqlite::params![
                    root.id,
                    seen.rel_path,
                    seen.kind.as_str(),
                    seen.format,
                    size,
                    seen.mtime,
                    pages,
                ],
            )
            .map_err(|e| format!("updating {}: {e}", seen.rel_path))?;
        }
        (false, _) => {
            report.added += 1;
            let pages = pages_of(root, seen);
            conn.execute(
                "INSERT INTO scan_entry
                     (root_id, rel_path, kind, format, size, mtime, pages)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    root.id,
                    seen.rel_path,
                    seen.kind.as_str(),
                    seen.format,
                    size,
                    seen.mtime,
                    pages,
                ],
            )
            .map_err(|e| format!("recording {}: {e}", seen.rel_path))?;
        }
    }

    match seen.kind {
        Kind::Dir => report.dirs += 1,
        Kind::File => report.files += 1,
    }
    if let (Kind::Dir, Some(count)) = (seen.kind, seen.entry_count) {
        conn.execute(
            "INSERT INTO dir_state (root_id, rel_path, mtime, entry_count, last_walked)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(root_id, rel_path) DO UPDATE SET
                 mtime = excluded.mtime,
                 entry_count = excluded.entry_count,
                 last_walked = excluded.last_walked",
            rusqlite::params![root.id, seen.rel_path, seen.mtime, count],
        )
        .map_err(|e| format!("recording the state of {}: {e}", seen.rel_path))?;
    }
    Ok(())
}

/// A page count for an entry that is new or has moved, or `None` where one
/// does not apply.
///
/// A directory's count comes from the walk, which summed it on the way past.
/// A file's costs an archive open, so it is only paid for a format this
/// project can actually read — a PDF or an EPUB is recorded with its format
/// and no count, which is what `a_scan_records_every_entry_with_the_format_
/// its_bytes_say` pins.
///
/// **A container that will not open is not an error.** `member_pages` refuses
/// a truncated zip, and a scan that aborted on one would leave a whole library
/// unscanned because of a single bad download. The count stays `None` and the
/// row still records what the bytes said the file was.
fn pages_of(root: &Root, seen: &Seen) -> Option<u32> {
    match seen.kind {
        Kind::Dir => seen.pages,
        Kind::File if seen.format == Some("zip") => {
            taiyaku_read::member_pages(&root.path.join(&seen.rel_path)).ok()
        }
        Kind::File => None,
    }
}

/// Drop rows for this root that the walk did not meet, and say how many.
///
/// Reached only after the §8.1 refusal above, so "the walk met nothing" can
/// never arrive here.
fn forget_unseen(conn: &Connection, root_id: i64, seen: &[Seen]) -> Result<usize, String> {
    let kept: Vec<String> = seen.iter().map(|entry| entry.rel_path.clone()).collect();
    let mut gone = 0;
    let stale: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT rel_path FROM scan_entry WHERE root_id = ?1")
            .map_err(|e| format!("listing what this root held: {e}"))?;
        let rows = stmt
            .query_map([root_id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("listing what this root held: {e}"))?;
        rows.filter_map(Result::ok)
            .filter(|held| !kept.contains(held))
            .collect()
    };
    for rel_path in stale {
        conn.execute(
            "DELETE FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root_id, rel_path],
        )
        .map_err(|e| format!("forgetting {rel_path}: {e}"))?;
        conn.execute(
            "DELETE FROM dir_state WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root_id, rel_path],
        )
        .map_err(|e| format!("forgetting the state of {rel_path}: {e}"))?;
        gone += 1;
    }
    Ok(gone)
}
```

`measure` is `inventory::totals_at`'s shape with `MAX_DEPTH = 32` and the same measured reason in its doc comment — `Path::is_dir()` swallows `ELOOP` and answers `false`, so past roughly 40 levels a cycle is silently truncated rather than crashing, and the ceiling has to fire first. Copy that reasoning across; do not restate it as "a sensible limit". It returns the tree; `flatten` walks that tree into the `Vec<Seen>` the loop above consumes, in the same order, with the root itself as `rel_path` `""`.

**A page image inside a directory member is counted, never recorded** — `taiyaku_read::is_page_name` is the test, the same function `read_edition` uses. Recording one row per page would put hundreds of thousands of rows in `scan_entry` for a real library to hold facts derivation never reads one at a time.

Dotfiles are skipped, exactly as `inventory` skips them — which is also what keeps `watch.rs`'s probe file out of the facts.

`add_root` canonicalises the path it is given before storing it, so `rel_path` is computed against a stable root and two spellings of one directory cannot become two roots. `taiyaku-cli`'s `canonical_sidecar_path` is the existing precedent.

In `crates/taiyaku-library/src/lib.rs`, add `pub mod scan;`. `rusqlite::OptionalExtension` needs importing for the `.optional()` above.

- [ ] **Step 3c: Four corrections from the review, and what they replace**

*The code above is kept as written and superseded here, per this project's convention. All four findings are defects in the plan rather than in the implementation of it, and three of them are the same defect wearing different clothes: a claim in a doc comment that the code beside it does not honour.*

**Correction 1 — a directory's recursive total goes stale on every ancestor.** `record`'s unchanged branch touches `last_seen` and nothing else, so a directory whose own `(size, mtime)` did not move keeps its stored `pages`. But a directory's mtime moves only when an entry is added, removed or renamed *in it*. **Measured on this filesystem on 2026-08-17:** adding a page to `series/vol01/` moved `vol01` from 60 to 80 bytes with a new mtime, and left `series` and the root byte-identical with unchanged mtimes. So every ancestor of a changed directory keeps its pre-change total forever — and `derive.rs` reads exactly those totals to pick the deepest directory that is actually a member. The correct total was already computed by the walk and was being discarded.

**Correction 2 — the cheap walk was not skipping the expensive call.** `measure_file` called `taiyaku_read::sniff` on every non-page file on every scan, before `record` looked at anything. `sniff` opens the file, and for anything with a zip header opens it a *second* time and parses the whole central directory. So "an entry is only reopened when its `(size, mtime)` has moved" was false for the normal case — a library of `.cbz` members — and the `(size, mtime)` gate was saving the second half of one archive's cost rather than all of it. Sniffing moves into `record`, where the gate already is.

**Correction 3 — one unreadable file aborted the whole scan.** `measure_file` propagated `metadata` and `sniff` failures with `?`, so a broken symlink, a file with no read permission, a FIFO, or a file removed between `read_dir` and `metadata` returned `Err` from `scan_root` and left the entire library unscanned. `pages_of` already refuses to cause that for a truncated archive — "a scan that aborted on one would leave a whole library unscanned because of a single bad download" — and the protection simply was not applied to the two calls that run on strictly more files.

**Correction 4 — no test reached the expensive path.** The fixture holds no archive, so `pages_of`'s zip arm was unreachable in the whole suite and `member_pages` was never called by any test in the module. An implementation that reopened every container on every scan would have passed. The fixture's own doc comment claimed the opposite.

`Seen` loses its `format` field entirely — a file's format is now decided in `record`, and a field that is only meaningful for one of two kinds is what produced defect 11:

```rust
struct Seen {
    rel_path: String,
    kind: Kind,
    size: u64,
    mtime: i64,
    /// Recursive page total for a directory; `None` for a file, whose count
    /// costs an archive open and is paid in `record`.
    pages: Option<u32>,
    entry_count: Option<i64>,
}
```

```rust
/// What this entry's bytes say it is.
///
/// **Called only when an entry is new or has moved**, which is what makes
/// this module's cheap-walk claim true rather than aspirational. `sniff`
/// opens the file, and for anything carrying a zip header opens it a second
/// time and parses the entire central directory — so calling it from the
/// walk, on every file, on every scan, was the expensive thing this module
/// says it avoids. It did exactly that until 2026-08-17.
///
/// A file whose bytes cannot be read is `None` rather than an error, for the
/// same reason [`pages_of`] swallows a refusal: one unreadable file must not
/// leave a whole library unscanned.
fn format_of(root: &Root, seen: &Seen) -> Option<&'static str> {
    match seen.kind {
        Kind::Dir => taiyaku_read::Format::Dir.token(),
        Kind::File => taiyaku_read::sniff(&root.path.join(&seen.rel_path))
            .ok()
            .and_then(taiyaku_read::Format::token),
    }
}

/// A page count for an entry that is new or has moved, or `None` where one
/// does not apply.
///
/// A directory's count comes from the walk, which summed it on the way past.
/// A file's costs an archive open, so it is only paid for a format this
/// project can actually read — a PDF or an EPUB is recorded with its format
/// and no count.
///
/// **A container that will not open is not an error.** `member_pages` refuses
/// a truncated zip, and a scan that aborted on one would leave a whole
/// library unscanned because of a single bad download. The count stays `None`
/// and the row still records what the bytes said the file was.
///
/// Takes the token [`format_of`] just produced rather than re-sniffing, and
/// compares against `Format::Zip.token()` rather than a `"zip"` literal —
/// that vocabulary is the schema's, and a second copy of it here is what
/// `Format::token`'s own doc warns against.
fn pages_of(root: &Root, seen: &Seen, format: Option<&'static str>) -> Option<u32> {
    match seen.kind {
        Kind::Dir => seen.pages,
        Kind::File if format == taiyaku_read::Format::Zip.token() => {
            taiyaku_read::member_pages(&root.path.join(&seen.rel_path)).ok()
        }
        Kind::File => None,
    }
}

/// A directory's recursive total, or `None` for a file.
///
/// Split out because the unchanged branch needs it and must not pay for a
/// file's count to get it.
fn dir_pages(seen: &Seen) -> Option<u32> {
    match seen.kind {
        Kind::Dir => seen.pages,
        Kind::File => None,
    }
}
```

`record`'s unchanged branch, which is where correction 1 lands:

```rust
(true, true) => {
    // `last_seen`, and for a directory its recursive page total.
    //
    // **A directory's total is written even when the directory itself has
    // not moved**, because a directory's mtime moves only when an entry is
    // added, removed or renamed *in it*. Measured on this filesystem on
    // 2026-08-17: adding a page to `series/vol01/` moved `vol01` from 60 to
    // 80 bytes with a new mtime and left `series` and the root
    // byte-identical. Without this, every ancestor of a changed directory
    // keeps its pre-change total forever, and `derive.rs` reads exactly
    // those totals to pick the deepest directory that is a member.
    //
    // It costs nothing: the walk summed it on the way past, so no container
    // is opened. `COALESCE` leaves a file's stored count alone.
    conn.execute(
        "UPDATE scan_entry SET last_seen = datetime('now'), pages = COALESCE(?3, pages)
          WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root.id, seen.rel_path, dir_pages(seen)],
    )
    .map_err(|e| format!("touching {}: {e}", seen.rel_path))?;
}
```

Both other branches compute the pair together, `let format = format_of(root, seen);` then `let pages = pages_of(root, seen, format);`, and bind `format` where they bound `seen.format`.

`measure_file` keeps only the `stat`, and a failure skips the entry:

```rust
/// One file's facts, or `None` if it cannot be stat'ed.
///
/// **A file that vanished or cannot be read is skipped, not an error.** A
/// broken symlink, a file removed between `read_dir` and this call, a FIFO,
/// or a file with no read permission would otherwise abort the scan of an
/// entire library — the same failure [`pages_of`] already refuses to cause
/// for a truncated archive. An `Err` from this module is reserved for a
/// *directory* that cannot be listed, which is a fact about the walk rather
/// than about one file.
///
/// It no longer sniffs. See [`format_of`].
```

Four cheap fixes from the same review, each one line:

- **`metadata(path)` before `read_dir(path)`** in `measure_dir`. A file landing in the directory mid-walk can be missed by `read_dir` while `metadata` picks up the *new* mtime, so the next scan sees the directory as unchanged and never notices. Stat first and the race fails safe.
- **`forget_unseen` builds a `HashSet`** rather than calling `Vec::contains` per held row.
- **The refused report says `dirs: 0`**, not `seen.len()`. Nothing was recorded, and `dirs: 1` reads as though one directory was.
- **The symlink test asserts `contains("nested past 32 directories deep")`**, not `contains("32")`. The loose form would pass on an unrelated `std::io` error whose path or pid contained those digits.

- [ ] **Step 3d: The two tests that would have caught corrections 1 and 4**

```rust
#[test]
fn a_page_added_two_levels_down_refreshes_every_ancestors_total() {
    // A directory's mtime does not move when anything happens below it, so
    // an ancestor's recursive total is only correct if the walk rewrites it
    // unconditionally. Measured: adding a page to `series/vol01/` leaves
    // `series` and the root byte-identical with unchanged mtimes.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");
    scan_root(&conn, &root).expect("first");

    let series = tree.path.join("Go-Toubun no Hanayome");
    std::fs::write(
        series.join("Go-Toubun no Hanayome - v01 (2019)").join("003.jpg"),
        b"one more page",
    )
    .expect("write");
    scan_root(&conn, &root).expect("second");

    let total: i64 = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(total, 8, "the container's total followed the page added below it");
}

#[test]
fn a_container_that_will_not_open_is_recorded_with_its_format_and_no_count() {
    // Reaches `pages_of`'s zip arm, which no other test in this module does
    // — the fixture holds no archive, so an implementation that reopened
    // every container on every scan would pass everything else here. Four
    // bytes of zip signature and nothing behind them: `sniff` answers
    // `Zip`, `member_pages` refuses, and the refusal is swallowed rather
    // than failing the scan.
    let tree = fixture();
    std::fs::write(tree.path.join("truncated.cbz"), b"PK\x03\x04").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");

    let report = scan_root(&conn, &root).expect("a bad archive is not a failed scan");

    assert_eq!(report.files, 2, "notes.pdf and the truncated archive");
    let (format, pages): (Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT format, pages FROM scan_entry WHERE rel_path = 'truncated.cbz'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select");
    assert_eq!(format.as_deref(), Some("zip"));
    assert_eq!(pages, None, "it opened, it refused, and the scan carried on");
}
```

The fixture's doc comment loses its last sentence. It claims `member_pages` "reads a directory member exactly as it reads an archive… so the fact layer is exercised the same way" — but `pages_of` returns `seen.pages` for a directory and never calls `member_pages` for one, so that sentence describes coverage that did not exist. Replace it with: *Archive reading is `taiyaku-read`'s to test; what this module owns is the dispatch, which `a_container_that_will_not_open_is_recorded_with_its_format_and_no_count` reaches.*

- [ ] **Step 3e: Take the filesystem out of the write transaction, and four smaller things**

*The second review approved the corrections above and found four more. The largest predates them and is the plan's, not the fix round's.*

**Correction 5 — `record` does filesystem I/O while holding the write lock.** `record` runs inside `tx`, and it calls `sniff` — one open, plus a full central-directory parse for anything with a zip header — and `member_pages`, another archive open, for every new-or-moved entry. `unchecked_transaction` issues a deferred `BEGIN`, but the first `record` writes immediately, so the RESERVED lock is taken at the top of the loop and held for the whole walk. `BUSY_TIMEOUT` is five seconds (`lib.rs`), and `taiyaku serve` writes a `progress` row on every page turn. A first scan of a real library — 156 series, every entry new, plausibly on network storage, which is exactly the deployment §8.1's unmounted-share scenario assumes — exceeds five seconds inside that lock without difficulty. That is the two-process "database is locked" failure this crate documents and engineered against, reintroduced by the one module that walks a filesystem.

**Resolve the expensive facts before `BEGIN`.** `scan_root` becomes: measure, refuse-if-empty, read the held `(rel_path → size, mtime)` map for this root in one query *outside* any transaction, decide which entries are new-or-moved, open and sniff **only those**, and only then open the transaction and write. All filesystem I/O moves ahead of the lock; the transaction becomes pure SQL over values already in hand.

```rust
/// What a new-or-moved entry's bytes turned out to say, resolved before the
/// write transaction opens.
///
/// **The whole point of this type is where it is built.** Sniffing and
/// counting pages are filesystem operations, and doing them inside `tx` held
/// the write lock across every archive open in a library scan — five seconds
/// is [`crate::BUSY_TIMEOUT`], and a first scan of 156 series on network
/// storage passes that without difficulty, while `taiyaku serve` is writing a
/// `progress` row on every page turn.
struct Resolved {
    format: Option<&'static str>,
    pages: Option<u32>,
}

/// The `(size, mtime)` this root already holds, by `rel_path`.
///
/// One query rather than one per entry, and outside the transaction, so the
/// decision about what needs opening is made before any lock is taken.
fn held_for(conn: &Connection, root_id: i64) -> Result<HashMap<String, (i64, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path, size, mtime FROM scan_entry WHERE root_id = ?1")
        .map_err(|e| format!("reading what this root held: {e}"))?;
    let rows = stmt
        .query_map([root_id], |r| {
            Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))
        })
        .map_err(|e| format!("reading what this root held: {e}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}
```

`record` then takes the held pair and the pre-resolved facts instead of querying and opening for itself:

```rust
fn record(
    conn: &Connection,
    root: &Root,
    seen: &Seen,
    held: Option<(i64, i64)>,
    resolved: Option<&Resolved>,
    report: &mut ScanReport,
) -> Result<(), String>
```

`resolved` is `Some` exactly when the entry is new or has moved — the same condition that decides which branch runs — so a mismatch between them is unrepresentable rather than merely avoided.

**Correction 6 — a FIFO can wedge a scan, and inside the lock.** `sniff` calls `File::open`, and opening a FIFO **blocks until a writer appears**; measured, `timeout 2 cat` on one exits 124. `measure_file` records a FIFO happily, because `stat` succeeds on it. Skip anything that is not a regular file or a directory:

```rust
// **Regular files and directories only.** `stat` succeeds on a FIFO, a
// socket and a device node, so the walk would record them — and then
// `sniff` would `File::open` a FIFO and block until a writer appeared,
// which measured as an indefinite hang. Nothing here can read one anyway.
if !meta.is_file() {
    return None;
}
```

**Correction 7 — two doc comments now say what the code no longer does**, which is the defect class corrections 1-4 were convened to remove:

- `record`'s summary says an unchanged entry "keeps its stored `pages` and `sha256` and is only touched to move `last_seen` forward". Since correction 1 that is false for directories. It becomes: *keeps its stored `sha256`, and a file its stored `pages`, and is otherwise touched only to move `last_seen` forward. A directory's total is rewritten either way — see the branch.* The same paragraph calls `member_pages` "the expensive call here"; `sniff` is the larger one for an archive, and both should be named.
- `measure_file`'s doc claims it skips a FIFO and a permission-denied file. Measured, it skips neither: `stat` succeeds on both. What it actually drops is a path that cannot be stat'ed — vanished, or a dangling symlink. Permission denial is handled downstream by `format_of`'s `.ok()`, and the FIFO by correction 6. Say that.

**Correction 8 — nothing observes that a container is *not* reopened.** No test scans an unchanged archive twice, so an implementation that re-sniffed every archive on every scan would pass all eight. The fix needs no `zip`: rewrite the archive with four different bytes of the same length and restore its mtime, so the entry reads as unchanged, then assert the stored format is *still* `"zip"` — which can only hold if `format_of` was not called.

```rust
#[test]
fn an_unchanged_container_is_not_reopened() {
    // The module's headline claim, made observable. `truncated.cbz` is
    // rewritten with four different bytes of the SAME length and its mtime
    // restored, so the `(size, mtime)` gate reads it as unchanged. If the
    // scan re-sniffed it, `format` would follow the new bytes to `pdf`.
    // `std::fs::FileTimes` is stable since 1.75, under this workspace's 1.85.
    let tree = fixture();
    let archive = tree.path.join("truncated.cbz");
    std::fs::write(&archive, b"PK\x03\x04").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");
    scan_root(&conn, &root).expect("first");

    let before = std::fs::metadata(&archive).expect("metadata");
    std::fs::write(&archive, b"%PDF").expect("rewrite, same length");
    let handle = std::fs::File::options()
        .write(true)
        .open(&archive)
        .expect("open to set times");
    handle
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(before.accessed().expect("atime"))
                .set_modified(before.modified().expect("mtime")),
        )
        .expect("restore mtime");

    let report = scan_root(&conn, &root).expect("second");

    assert_eq!(report.updated, 0, "size and mtime both unmoved");
    let format: Option<String> = conn
        .query_row(
            "SELECT format FROM scan_entry WHERE rel_path = 'truncated.cbz'",
            [],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(
        format.as_deref(),
        Some("zip"),
        "the bytes now say PDF; only a scan that did not reopen the file still says zip"
    );
}
```

Note while you are there that `a_container_that_will_not_open_is_recorded_with_its_format_and_no_count`'s `assert_eq!(pages, None)` cannot distinguish "took the zip arm and swallowed the refusal" from "never took the zip arm" — both give `None`. Its `format == "zip"` assertion is what carries it. Leave the assertion, add a comment saying which half does the work.

**Four one-line fixes from the same review:**

- **`Seen.entry_count` gets its doc comment back.** Step 3c dropped it. It read: *Directories only: how many entries `read_dir` returned, for `dir_state`. See the module doc for what this does and does not license skipping.* That is the one field whose misuse is this module's worst failure mode, and the pointer to the module doc should not have gone.
- **`scan_root`'s `# Errors` says "on any directory in the tree"**, not "on `root.path` itself" — any directory that cannot be listed aborts, and the narrow wording now contradicts `measure_file`'s own doc.
- **`a_page_added_two_levels_down_refreshes_every_ancestors_total` checks the root's row too** — `rel_path = ''`, expected 10 — since the test is named for every ancestor and checks one.
- **The module doc names `sniff` beside `member_pages`** as the expensive calls, so it agrees with `format_of`'s doc about which is larger.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p taiyaku-library scan::`
Expected: PASS — the six from Step 3, the two from Step 3d, and the one from Step 3e: nine in all.

- [ ] **Step 5: Gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "The walk: what is on disk, recorded, and an empty root refused rather than applied"
```

---

## Task 6: The rule engine

§4. Four kinds, ordered layering per field, and provenance — which rule decided each field — because §4 makes that a requirement on spec C4 and a value with no provenance makes the layering unexplainable.

**Files:**
- Create: `crates/taiyaku-library/src/rules.rs`, `crates/taiyaku-library/src/rules/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs`, `crates/taiyaku-library/Cargo.toml`

**Interfaces:**
- Consumes: `taiyaku_read::{chapter_number, volume_number}` (Task 1), `scan::Root` (Task 5)
- Produces:
  - `rules::Rule` — a projection of one `rule` row
  - `rules::Field` — `Series | EditionKey | Kind | Language | Volume | Chapter | Title`
  - `rules::Resolved { series, edition_key, kind, language, volume: Option<f64>, chapter: Option<f64>, title, provenance: BTreeMap<Field, i64> }`
  - `rules::Engine::load(conn: &Connection) -> Result<(Engine, Vec<String>), String>` — the compiled rules, and one complaint per pattern that would not compile
  - `Engine::resolve(&self, root: &Root, rel_path: &str) -> Resolved`

- [ ] **Step 0: `builtin-volume` needs a marker-required parser, and Task 1 only closed this in one direction**

*Added 2026-08-17, when this task's own `a_chapter_file_gets_a_chapter_and_no_volume` failed against already-shipped code. It is the exact mirror of the decision Task 1 made for `chapter_number`, asked of the other parser and never answered.*

`volume_number` **falls back to the first bare digit run** when no `v`/`vol`/`第` marker appears. That is right for its original caller: `inventory` hands it a directory that is a volume by construction, so a bare `069` is a volume number. Seeded rule 2 applies it to **every filename**, so `Kaguya-sama - c001.cbz` comes back as volume 1. Task 1's `chapter_number` doc states this hazard precisely — "a chapter parser with the same fallback would answer for every volume file in a library… and the scanner's `builtin-chapter` rule would then set `chapter` on everything it saw" — and closes it for chapters only.

**`volume_number` does not change.** Its fallback is measured behaviour with a shipped caller, and this project does not edit a measured function to suit a new consumer. What the scanner needs is a second entry point, and the shape it needs already exists — `chapter_number` is a marker-required, word-boundary-guarded scan. Factor that scan out and give it two marker sets:

```rust
/// A marker that introduces a number, and the suffix that must follow the
/// digits for it to count.
///
/// **`after` exists for `第` alone, and it is not optional decoration.** `第`
/// is an ordinal prefix, not a unit: `第69巻` is volume 69 and `第7話` is
/// chapter 7, and the counter that disambiguates them — `巻` for volumes,
/// `話` or `章` for chapters — comes *after* the digits. With `第` in both
/// marker sets and nothing looking past the number, every Japanese-named file
/// under the seeded defaults took a volume number and a chapter number with
/// the same value: measured, `ワンピース/第69巻.cbz` resolved to
/// `vol=Some(69.0) chap=Some(69.0)`. That is the defect Step 0 exists to
/// close, reached from the other side.
///
/// A bare `第7` with no counter is genuinely ambiguous and answers neither. A
/// layout that writes one is served by a `regex` rule, which is what the rule
/// layer's escape hatch is for.
struct Marker {
    before: &'static str,
    after: Option<&'static str>,
}

/// Volume markers, longest form first so `vol.` is not read as `vol`.
const VOLUME_MARKERS: [Marker; 4] = [
    Marker { before: "vol.", after: None },
    Marker { before: "vol", after: None },
    Marker { before: "v", after: None },
    Marker { before: "第", after: Some("巻") },
];

/// Chapter markers. `c` and `ch` are short enough to need the word-boundary
/// test in [`marked_number`]; see its doc. `話` and `章` both count chapters.
const CHAPTER_MARKERS: [Marker; 9] = [
    Marker { before: "chapter", after: None },
    Marker { before: "chp.", after: None },
    Marker { before: "chp", after: None },
    Marker { before: "ch.", after: None },
    Marker { before: "ch", after: None },
    Marker { before: "c.", after: None },
    Marker { before: "c", after: None },
    Marker { before: "第", after: Some("話") },
    Marker { before: "第", after: Some("章") },
];

/// The first digit run introduced by one of `markers`, with an optional
/// decimal part, and **no fallback**.
///
/// The scan `chapter_number` has always used, with the marker set lifted out
/// so `volume_number_marked` can share it rather than carry a second copy.
///
/// **A marker must start a word.** `c` and `v` are one character, so without
/// this they match the tail of ordinary words: `Scan by ABC 2019` reads as
/// 2019 because `abc` ends in `c`, and `Series Nov 7` reads as 7 because
/// `nov` ends in `v`. Both measured on the built function.
///
/// ASCII-only, because every kanji is alphanumeric to Unicode and
/// `単行本第7話` has to keep working.
///
/// `f64` because both callers want one: `member.chapter_declared` is `REAL`
/// for half-chapters, and `work.volume` is `REAL` because — in that column's
/// own words — "omnibus and half-volumes exist".
fn marked_number(name: &str, markers: &[&str]) -> Option<f64>
```

```rust
/// The volume number in a name, **only where a marker says so**.
///
/// [`volume_number`] directly above falls back to the first bare digit run
/// when no marker appears, which is right for `inventory` — it is handed a
/// directory that is a volume by construction. It is wrong for the scanner's
/// `builtin-volume` rule, which is applied to every filename in a library:
/// with the fallback, `Kaguya-sama - c001.cbz` comes back as volume 1, and
/// every chapter file in the corpus acquires a volume number it does not
/// have.
///
/// So this is the same question `chapter_number` answers for chapters, and
/// the same answer. The two functions are one scan over two marker sets; see
/// [`marked_number`].
///
/// A layout whose volumes carry no marker at all is served by a `regex` rule,
/// which is what the rule layer's escape hatch is for — not by loosening this.
pub fn volume_number_marked(name: &str) -> Option<f64> {
    marked_number(name, &VOLUME_MARKERS)
}
```

`chapter_number` becomes `marked_number(name, &CHAPTER_MARKERS)` and keeps its own doc comment, which explains the no-fallback decision at length and is still exactly right.

`marked_number` takes `&[Marker]` and, after consuming the digits and any decimal part, requires `after` to be the next thing in the name where the marker names one. The word-boundary test on `before` is unchanged.

The four cases that pin it, beside the existing ones:

```rust
#[test]
fn a_kanji_marker_needs_its_counter_and_the_counter_decides_which_number_it_is() {
    // `第` is an ordinal prefix, not a unit. Before the counter was checked,
    // both parsers answered for both forms and every Japanese-named file
    // took a volume and a chapter with the same value.
    assert_eq!(volume_number_marked("第69巻"), Some(69.0));
    assert_eq!(chapter_number("第69巻"), None);

    assert_eq!(chapter_number("第7話"), Some(7.0));
    assert_eq!(volume_number_marked("第7話"), None);

    assert_eq!(chapter_number("第12章"), Some(12.0));
    assert_eq!(volume_number_marked("第12章"), None);

    // Ambiguous without a counter: neither answers, and a `regex` rule is
    // the documented way to read a layout that writes this.
    assert_eq!(volume_number_marked("第7"), None);
    assert_eq!(chapter_number("第7"), None);
}
```

And one in `rules/tests.rs`, beside `a_chapter_file_gets_a_chapter_and_no_volume`, because nothing in that module resolves a Japanese path through the engine at all:

```rust
#[test]
fn a_japanese_volume_gets_a_volume_and_not_also_a_chapter() {
    let engine = engine_of(&defaults());
    let resolved = engine.resolve(&root(), "ワンピース/第69巻.cbz");
    assert_eq!(resolved.volume, Some(69.0));
    assert_eq!(resolved.chapter, None, "巻 counts volumes, not chapters");
}
```

**Two properties to confirm by running, not by reading**, because this edits a module whose behaviour is measured:

1. Every existing `volume_number` test still passes, unchanged. `volume_number` itself must be byte-identical — the boundary guard goes only into `marked_number`, which it does not call.
2. `volume_number_marked` agrees with `volume_number` on every name in the existing test set *except* the bare-digit ones, where it answers `None`: `volume_number_marked("069")` is `None` while `volume_number("069")` is `Some(69)`. That difference is the entire point, and it wants a test saying so.

`rules.rs`'s `BuiltinVolume` arm then calls `volume_number_marked`, and `Resolved.volume` takes its `f64` directly rather than converting.

- [ ] **Step 1: Add `regex`**

In `crates/taiyaku-library/Cargo.toml`:

```toml
# §4 of the scanner spec: the escape hatch for a layout the four rule kinds
# cannot express. Named here rather than slipped in — `docs/status.md` states
# this workspace's short dependency list as a fact about the project.
#
# **User-supplied patterns are safe here in a way they would not be
# elsewhere.** Rust's `regex` has no backtracking and matches in time linear in
# the input, so a pattern typed into C4's text box cannot produce catastrophic
# behaviour. "Accept a user regex" is normally a ReDoS hazard; the reason it is
# not one here is a property of this dependency rather than of the design.
regex = "1.13"
```

**It costs no new crate.** `regex` 1.13.1 is already in `Cargo.lock` — checked on 2026-08-17 — pulled in transitively by an existing dependency, so naming it here adds a dependency *edge* to something the workspace already compiles rather than a download and a build. That is worth knowing before weighing this against `inventory::volume_number`'s original "regex is three crates" objection: that objection was written when it would have been three new crates, and it is no longer.

- [ ] **Step 2: Write the failing tests**

`crates/taiyaku-library/src/rules/tests.rs`. The two that carry the design:

```rust
#[test]
fn a_later_rule_overrides_only_the_fields_it_sets() {
    // §4's headline case: "everything under this root is raw" and "except
    // these are the colour edition" are TWO rules, not four. The second sets
    // `edition_key` and says nothing about language, so the first's language
    // survives.
    let engine = engine_of(&[
        rule(1, "prefix", "", "path")
            .sets_edition_key("raw")
            .sets_language("ja"),
        rule(2, "prefix", "Full Color", "parent").sets_edition_key("colour"),
    ]);

    let plain = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(plain.edition_key.as_deref(), Some("raw"));
    assert_eq!(plain.language.as_deref(), Some("ja"));

    let colour = engine.resolve(&root(), "Full Color/Series - v01.cbz");
    assert_eq!(colour.edition_key.as_deref(), Some("colour"));
    assert_eq!(
        colour.language.as_deref(),
        Some("ja"),
        "the later rule set no language, so the earlier one still decides it"
    );
    assert_eq!(colour.provenance[&Field::EditionKey], 2);
    assert_eq!(colour.provenance[&Field::Language], 1);
}

#[test]
fn the_four_defaults_read_a_real_library_path() {
    let engine = engine_of(&defaults());
    let resolved = engine.resolve(&root(), "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019).cbz");

    // Rule 1 takes the directory; rule 4 layers the filename capture over it.
    assert_eq!(resolved.series.as_deref(), Some("Go-Toubun no Hanayome"));
    assert_eq!(resolved.provenance[&Field::Series], 4);
    assert_eq!(resolved.volume, Some(1.0));
    assert_eq!(
        resolved.chapter, None,
        "no chapter marker, and `chapter_number` does not fall back"
    );
}

#[test]
fn a_chapter_file_gets_a_chapter_and_no_volume() {
    let engine = engine_of(&defaults());
    let resolved = engine.resolve(&root(), "Kaguya-sama - Fan Translated/Kaguya-sama - c001.cbz");
    assert_eq!(resolved.chapter, Some(1.0));
    assert_eq!(resolved.volume, None);
}

#[test]
fn a_pattern_that_will_not_compile_is_reported_and_disables_only_itself() {
    // A bad pattern reaches this engine from a text box in C4's editor. It
    // must not take a scan of 156 series down with it.
    let (engine, complaints) = Engine::compile(&[
        rule(1, "regex", "^(?<series>[^/]+)/", "path"),
        rule(2, "regex", "(unclosed", "name"),
    ]);
    assert_eq!(complaints.len(), 1);
    assert!(complaints[0].contains("rule 2"), "{}", complaints[0]);
    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("Series"));
}
```

The helpers those tests use, written once at the top of the same file:

```rust
/// One rule row, with the `set_` columns filled in by the builder methods.
fn rule(ordinal: i64, kind: &str, pattern: &str, match_on: &str) -> Rule {
    Rule {
        id: ordinal,
        ordinal,
        enabled: true,
        root_id: None,
        kind: kind.parse().expect("a known rule kind"),
        pattern: pattern.to_string(),
        match_on: match_on.parse().expect("a known match_on"),
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: None,
        note: None,
    }
}

impl Rule {
    #[cfg(test)]
    fn sets_edition_key(mut self, key: &str) -> Self {
        self.set_edition_key = Some(key.to_string());
        self
    }
    #[cfg(test)]
    fn sets_language(mut self, language: &str) -> Self {
        self.set_language = Some(language.to_string());
        self
    }
}

/// An engine over rules that are all expected to compile.
fn engine_of(rules: &[Rule]) -> Engine {
    let (engine, complaints) = Engine::compile(rules);
    assert!(complaints.is_empty(), "{complaints:?}");
    engine
}

/// A root with nothing configured on it, so the rules alone decide.
fn root() -> Root {
    Root {
        id: 1,
        path: PathBuf::from("/library"),
        edition_key: None,
        default_kind: None,
        default_language: None,
        scan_interval_secs: None,
    }
}

/// §4's four seeded defaults, as the engine sees them after `load`.
fn defaults() -> Vec<Rule> {
    vec![
        rule(1, "regex", "^(?<series>[^/]+)/", "path"),
        rule(2, "builtin-volume", "", "name"),
        rule(3, "builtin-chapter", "", "name"),
        rule(4, "regex", "^(?<series>.+?)[ _-]+v", "name"),
    ]
}
```

`Engine::compile(&[Rule])` — taking rules already loaded — is what makes every one of these tests independent of SQLite. `Engine::load(conn)` is the thin wrapper that reads the rows and calls it, the same split as `require_fts5` / `require_fts5_with_probe` in `lib.rs` and for the same reason: the branch worth pinning is the one a database cannot easily be made to produce.

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library rules::`
Expected: FAIL — the module does not exist.

- [ ] **Step 4: Write `rules.rs`**

The engine compiles every `regex` pattern once at load. Compiling per entry would recompile four patterns for each of a library's thousands of files, and `Regex::new` is the expensive call in this crate by an order of magnitude.

```rust
/// Every enabled rule, compiled, in `ordinal` order — and one complaint per
/// pattern that would not compile.
///
/// **A bad pattern disables its own rule and nothing else.** These arrive from
/// a text box (spec C4's editor) and from imported JSON, so a scan of a real
/// library must not die because one rule is malformed. The complaints are
/// returned rather than logged, so the caller — `library scan`, and later the
/// API — decides how loudly to say so; a rule silently doing nothing is the
/// outcome this avoids.
///
/// # Errors
///
/// Returns an error only if SQLite refuses the query. A malformed pattern is a
/// complaint, not an error.
pub fn load(conn: &Connection) -> Result<(Engine, Vec<String>), String>
```

`resolve` walks the compiled rules in order and, for each that matches, sets the fields it provides:

- `Prefix` — `pattern` is a path prefix, tested against whichever of `path`, `name` or `parent` `match_on` names. Sets only the `set_*` columns. This is the one-click case: "this whole directory is raw", no pattern to write.
- `Regex` — sets the `set_*` columns, then whichever of the named captures `series`, `volume`, `chapter`, `edition`, `title` are present. `volume` and `chapter` parse as `f64`; a capture that will not parse sets nothing and is not an error, since a rule matching a name whose digits are not a number is the rule being wrong about that file rather than the library being broken.
- `BuiltinVolume` — `taiyaku_read::volume_number` on the `match_on` string, into `volume`.
- `BuiltinChapter` — `taiyaku_read::chapter_number`, into `chapter`.

Every set records `provenance.insert(field, rule.id)`, overwriting — so the map ends holding, per field, the id of the last rule that decided it. That is exactly what C4 has to display.

Fields with no rule at all fall back to the root's own `edition_key`, `default_kind` and `default_language` columns, which §3 of the store spec describes as "what this root's material becomes by default; B's rules layer over them". Record that fallback in `provenance` as the sentinel `0`, so "the root decided this" is distinguishable from "nothing decided this".

Add `pub mod rules;` to `lib.rs`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p taiyaku-library rules::`
Expected: PASS, all four.

- [ ] **Step 6: Gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "The rule layer: four kinds, layered per field, and a bad pattern that disables only itself"
```

---

## Task 7: Derivation, and the re-derive that never touches the disk

§2's claim, §7's model, and the test that proves the claim by deleting the tree first. This is the task the whole architecture exists for.

It also edits `import.rs` a second time, for a reason nothing earlier could have surfaced: `member.chapter_declared` has **no representation in the sidecar format**. `taiyaku_core::Member` is `{ path, sha256, pages }` and nothing more, while `import::replace_editions` replaces a work's members wholesale from the document. So a chapter number the scanner recorded is erased by the next `align --write` over the same work — and the store spec put that column in migration 1 precisely because it is expensive to backfill, costing "a full re-read of every member".

**Files:**
- Create: `crates/taiyaku-library/src/derive.rs`, `crates/taiyaku-library/src/derive/tests.rs`
- Modify: `crates/taiyaku-library/src/import.rs` (preserve `chapter_declared` across a replace), `crates/taiyaku-library/src/lib.rs`
- Test: `crates/taiyaku-library/src/import/tests.rs`

**Interfaces:**
- Consumes: `scan::Root`, `rules::Engine`, `import::scanned` (Tasks 3, 5, 6)
- Produces:
  - `derive::DeriveReport { series: usize, works: usize, editions: usize, members: usize, unattached_chapter_dirs: usize, unreadable: usize }`
  - `derive::derive(conn: &Connection) -> Result<DeriveReport, String>`

- [ ] **Step 1: Write the failing test that is the whole point**

```rust
#[test]
fn editing_a_rule_re_derives_the_library_without_touching_the_filesystem() {
    // §2's claim, and the reason the fact layer is persisted at all. The tree
    // is DELETED between the scan and the second derive: if anything reopens
    // an archive to recount pages or rehash bytes, this fails.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let before: String = conn
        .query_row("SELECT key FROM edition LIMIT 1", [], |r| r.get(0))
        .expect("edition");
    assert_eq!(before, "raw");

    std::fs::remove_dir_all(&tree.path).expect("remove the tree entirely");

    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key)
         VALUES (99, 'prefix', '', 'path', 'colour')",
        [],
    )
    .expect("new rule");
    let report = derive::derive(&conn).expect("re-derive with no filesystem at all");

    assert_eq!(report.works, 2);
    let after: String = conn
        .query_row("SELECT key FROM edition LIMIT 1", [], |r| r.get(0))
        .expect("edition");
    assert_eq!(after, "colour");
}

#[test]
fn a_chapter_organised_directory_produces_members_and_no_work() {
    // §7. Which seven of 146 chapter files constitute volume 3 is a question
    // only the alignment answers, so the members are recorded with
    // `chapter_declared` and the directory stays visibly unattached.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 2, "the two volumes, and not the chapter file");
    assert_eq!(report.unattached_chapter_dirs, 1);
}

#[test]
fn re_deriving_keeps_a_series_row_so_a_dismissal_still_points_at_something() {
    // `proposal.subject_id` keys on the series row. If derivation deleted and
    // re-inserted series, every dismissal would be orphaned on the next scan
    // and §6's "dismissal has to persist" would quietly stop being true.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let first: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("series");

    derive::derive(&conn).expect("second");

    let second: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("series");
    assert_eq!(first, second, "the same row, reconciled rather than re-minted");
}
```

`fixture()` is Task 5's, unchanged. Make `scan::tests::fixture` and `write_pages` `pub(crate)` there rather than copying them — two fixtures describing the same tree drift, and the whole point of these tests is that both modules see the *same* facts.

The edition key those tests expect is `raw`, which needs a home:

```rust
/// The edition key a work gets when nothing sets one.
///
/// Reached when neither a rule nor the root's own `edition_key` column
/// decides — a root someone added and has not configured. `raw` rather than
/// something neutral like `default` because it is what an unconfigured root
/// holding one edition per work actually is in this corpus, and because a
/// reader meets this string: it is the edition name in the swap UI. C4's root
/// editor is what changes it, and a rule overrides it per path.
const DEFAULT_EDITION_KEY: &str = "raw";
```

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library derive::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write `derive.rs`**

Its shape, and the two decisions worth stating in the doc comment:

```rust
//! `scan_entry` + `rule` → series, works, editions, members.
//!
//! **Exactly one input, which is what makes §2's claim true.** Structure comes
//! from the facts and the rules and from nothing else — not from a manual
//! accept, which §6.1 routes through a written `rule` for precisely this
//! reason. Two inputs would mean a re-derive either discards the accepts or
//! needs a merge rule to protect them, which is the problem the store spec
//! already solved once with `by: "hand"` and `Sidecar::merge`; solving it a
//! second time in a different shape is how two mechanisms come to disagree.
//!
//! **It never opens a file.** Page counts and digests are columns on
//! `scan_entry`, written by the walk, so editing a rule re-derives 156 series
//! at memory speed. `derive/tests.rs` deletes the tree before the second
//! derive, which is the only way to test a negative like this.
```

The body, per §7:

1. Load the rules once (`rules::Engine::load`), and the roots.
2. For every `scan_entry` whose format `is_readable`, resolve it. Unreadable formats are counted into `report.unreadable` and produce no structure — recorded, not skipped, exactly as the walk recorded them.
3. **Fill in `sha256` where it is NULL**, which is the one moment §3's laziness ends: a member about to become part of a work needs its digest, from `taiyaku_read::member_digest`, written back to `scan_entry` so the next derive does not need the file. An entry whose file has gone and whose hash is NULL is skipped with a count, not an error — that is the deleted-tree case in the test above, and it must not be a failure for entries that were never derived.
4. Group the resolved entries by `(series, volume)`. Each group is one work; each distinct `edition_key` within it is one edition; each entry is one member, ordered by `natural_cmp` on `rel_path`.
5. Entries with a `chapter` and no `volume` are §7's chapter case: their directory produces **members and no work**. Count the directory into `unattached_chapter_dirs` and leave Task 8 to raise the `attach-chapters` proposal.
6. Build a `Sidecar` per work and hand it to `import::scanned`:

```rust
let doc = Sidecar {
    taiyaku: taiyaku_core::VERSION,
    work: Work {
        title: title.clone(),
        // Every edition in this corpus is manga, and neither `root` nor
        // `rule` carries a reading direction to set. A rule field for it is
        // spec C4's business; until something in the library reads
        // left-to-right this is a default rather than a guess, and it is
        // recorded here so the next person does not read it as one.
        reading: Direction::RightToLeft,
    },
    editions,
    hub: hub_key,
    alignments: Vec::new(),
};
let work_id = crate::import::scanned(conn, &doc)?;
```

**The hub is fixed when the work is created, and re-read from the row afterwards.** `identity.rs`'s own doc comment says so in as many words, and it is load-bearing: `content_key` is a hash over *the hub edition's* members, so a work whose hub changes gets a different key, and `import` then mints a second row and orphans the first — with the reader's progress and notes on it.

*A first draft of this step said "the hub is the edition whose `edition_key` sorts first among the work's, which makes it stable across re-derives". That is wrong, and wrong in the exact way spec A's §7 correction exists to prevent — it is that defect arriving by a different door. Sorting first is stable only while the set does not change: the day a second edition of an existing work turns up in another root with a key sorting before the current hub, every one of those works changes identity at once. Kept and marked rather than deleted, because "stable" was asserted about a rule that is not.*

So derivation resolves the hub in two steps:

```rust
// Look the work up by (series, volume) — columns migration 1 already
// carries — rather than by content_key, which cannot be computed until the
// hub is known. This is the one lookup that breaks that circle.
let existing_hub: Option<String> = conn
    .query_row(
        "SELECT hub FROM work WHERE series_id = ?1 AND volume IS ?2",
        rusqlite::params![series_id, volume],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up an existing work's hub: {e}"))?;
// Only a genuinely new work chooses one, and then the choice is arbitrary
// but permanent: whichever key sorts first among the editions it has today.
let hub_key = existing_hub
    .filter(|held| editions.contains_key(held))
    .unwrap_or_else(|| editions.keys().next().cloned().expect("one edition"));
```

`volume IS ?2` rather than `=`, because `volume` is nullable and `NULL = NULL` is NULL in SQL — a chapter-organised work would never match itself.

The `.filter(|held| editions.contains_key(held))` arm matters: if the edition that *was* the hub has since disappeared from disk, its members are gone and the old `content_key` cannot be reproduced anyway, so a new hub is chosen and a new work minted. That is the honest outcome, and it is worth a test of its own:

```rust
#[test]
fn a_works_hub_survives_a_second_edition_arriving_in_another_root() {
    // The identity hazard, from the side that would bite: adding an edition
    // whose key sorts BEFORE the hub must not change what the work is.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let (id, hub): (i64, String) = conn
        .query_row("SELECT id, hub FROM work ORDER BY id LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("work");
    assert_eq!(hub, "raw");

    // A second root whose rule names an edition key sorting before "raw".
    let second = crate::tests::tempdir("derive-second-root");
    // ... same two volumes, and a rule setting edition_key to "colour" ...
    let other = scan::add_root(&conn, &second.path).expect("add");
    scan::scan_root(&conn, &other).expect("scan");
    derive::derive(&conn).expect("second");

    let (still, hub_now): (i64, String) = conn
        .query_row("SELECT id, hub FROM work WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the same work");
    assert_eq!(still, id);
    assert_eq!(hub_now, "raw", "the hub was fixed when the work was created");
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2, "two volumes, not four");
}
```

7. **Series are reconciled, never re-minted.** `INSERT INTO series (sort_title) ... ON CONFLICT DO NOTHING` then `SELECT id`, because `proposal.subject_id` points at these rows and re-minting them orphans every dismissal.

Add `pub mod derive;` to `lib.rs`.

- [ ] **Step 4: Run the derive tests**

Run: `cargo test -p taiyaku-library derive::`
Expected: PASS, all three.

- [ ] **Step 5: Write the failing test for `chapter_declared` surviving an align**

In `crates/taiyaku-library/src/import/tests.rs`:

```rust
#[test]
fn a_chapter_number_the_scanner_recorded_survives_a_later_align() {
    // `taiyaku_core::Member` is { path, sha256, pages } — the sidecar format
    // has nowhere to carry `chapter_declared`, and `replace_editions` replaces
    // members wholesale from the document. Without preservation, the first
    // `align --write` over a scanned chapter edition erases a fact that costs
    // a full re-read of every member to recover, which is exactly why the
    // store spec put the column in migration 1.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = scanned(&conn, &doc("aa")).expect("scan");
    conn.execute(
        "UPDATE member SET chapter_declared = 1.0, chapter_source = 'filename'",
        [],
    )
    .expect("record the chapter");

    sidecar(&conn, &doc("aa"), Path::new("/library/series/taiyaku.json")).expect("align");

    let (chapter, source): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT chapter_declared, chapter_source FROM member
             JOIN edition ON edition.id = member.edition_id WHERE edition.work_id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select");
    assert_eq!(chapter, Some(1.0));
    assert_eq!(source.as_deref(), Some("filename"));
}
```

Run: `cargo test -p taiyaku-library import::tests::a_chapter_number_the_scanner`
Expected: FAIL — `chapter` is `None`, the replace having dropped it.

- [ ] **Step 6: Preserve it across the replace**

In `replace_editions`, read the existing `(edition key, ordinal) → (chapter_declared, chapter_source)` pairs into a map before deleting, and write them back onto the rows that land in the same positions. Give it a comment saying what it is for and what it is not:

```rust
// **Carried across the replace, because the document cannot carry it.**
// `member.chapter_declared` is written by the scanner from a filename or
// from ComicInfo.xml and has no field in `taiyaku.json` — `Member` is
// { path, sha256, pages } — so a wholesale replace from a document would
// erase it. Matched on (edition key, ordinal) rather than on path: a
// re-solve that renames nothing keeps both, and one that reorders members
// has changed what chapter each position holds anyway, so carrying by
// position is the honest answer rather than the convenient one.
```

- [ ] **Step 7a: A rescan must not delete what derivation has no opinion about**

*Settled by the owner on 2026-08-17, after the implementer recorded the problem in code and declined to work around it. Two of this task's earlier steps were also wrong about the code and are corrected below.*

**The defect.** `import::scanned` passes `alignments: Vec::new()`, and `import_with` calls `replace_alignments`, which replaces wholesale. So: scan a library, align a volume with `align --write`, rescan — and the `alignment` and `run` rows are deleted. `page_flag` goes the same way, taking `credits --write`'s output and any recorded spreads with it. The file on disk survives and a `serve` restart re-imports it, but a `library export` in the window between writes a sidecar with no alignments **over the reader's own copy**.

**The rule: derivation owns structure derived from facts and rules, and nothing else.** Which files are members of which edition of which work is its business. Which pages are credits, which are spreads, and how two editions correspond are not — those come from a solver or from a person, and the module that walks a filesystem should not be the thing that deletes them.

So `scanned` — and only `scanned`, not `sidecar` — leaves both alone:

```rust
/// **A scan replaces editions and members. It does not touch alignments or
/// page flags**, which is the difference between this and [`sidecar`].
///
/// Derivation owns what the facts and the rules decide: which files are
/// members of which edition of which work. It has no opinion about which
/// pages are credits, which are spreads, or how two editions correspond —
/// those come from `align`, from `credits`, or from a person, and a module
/// that walks a filesystem must not be the thing that deletes them. Before
/// 2026-08-17 it was: a rescan after an `align --write` dropped the work's
/// `alignment` and `run` rows, and a `library export` in the window then
/// wrote a sidecar with no alignments over the reader's own copy.
///
/// This extends a principle the store already had rather than inventing one:
/// [`replace_editions`] carries `chapter_declared` across its own wholesale
/// replace, keyed on `sha256`, for exactly this reason.
```

`replace_editions` gains the same treatment for `page_flag` that it already gives `chapter_declared`: read the held `(edition key, kind, page)` rows before deleting, and restore them for editions that still exist. Keyed on the edition's **key**, not its row id, since the rows are replaced.

**What this costs, stated rather than discovered.** If a rule edit removes an edition an alignment references, that edge dangles — it names an edition the document no longer holds. `taiyaku-core` resolves every edge with `.find()`/`.position()`, so a dangling edge fails to resolve rather than corrupting anything, and `Sidecar::quality_of` recomputes from runs so no stale verdict can mean anything. It is recoverable by re-solving. It wants a test that a work whose spoke edition disappears still projects, still serves its hub, and reports the edge as unresolvable rather than panicking.

- [ ] **Step 7b: A volume split into chapter subdirectories is one member, not eight**

`docs/sidecar.md` records **seven of fifty-seven corpus archives** putting chapters in subdirectories. The fact layer records both the container directory and each child, so "each entry is one member" makes such a volume produce a member for the container *and* one per chapter — counting its pages twice and inventing seven members that are not editions of anything.

`inventory::decide` solves the same problem with a page threshold, which derivation has no equivalent of. The rule layer gives a better one:

**A directory is a member only if no ancestor of it is also a member candidate.** A candidate is an entry the rules assigned a `volume` or a `chapter`. A series directory is not a candidate — the rules give it neither — so it does not suppress the volumes inside it; a volume directory is, so it suppresses its own chapter subdirectories. Implementable from `rel_path` prefixes alone, with no extra facts and no filesystem access.

```rust
#[test]
fn a_volume_split_into_chapter_subdirectories_is_one_member() {
    // `sidecar.md` records seven of fifty-seven corpus archives doing this.
    // Without the ancestor test the container and all three children become
    // members, the volume's pages are counted twice, and three chapters
    // become editions of nothing.
    let tree = crate::tests::tempdir("derive-nested");
    let volume = tree.path.join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        crate::scan::tests::write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.works, 1);
    assert_eq!(report.members, 1, "the volume, not its three chapters");
    let pages: i64 = conn
        .query_row("SELECT pages FROM member", [], |r| r.get(0))
        .expect("member");
    assert_eq!(pages, 6, "counted once");
}
```

- [ ] **Step 7d: A suppressed entry is counted, because nothing this project drops is dropped silently**

*Raised by the implementer after 7b landed, and it is right.* The ancestor test in 7b drops entries, and `DeriveReport` has no field saying how many. A rule set that accidentally made a series directory a candidate would suppress every volume under it and derive almost nothing, with no number anywhere saying that is what happened — it would read as a library that simply has few works.

That is the failure `scripts/check-all.sh` names in its own header: *"Anything not measured is named twice — once where it would have run, and again in the closing summary, which refuses to print a bare 'all green' when something was skipped."* A derive report is the same kind of document.

`DeriveReport` gains one field:

```rust
    /// Entries the rules made candidates but an ancestor candidate suppressed
    /// — a volume split into chapter subdirectories contributes one member
    /// and three of these.
    ///
    /// **Counted because a silent drop is indistinguishable from an empty
    /// library.** A rule set that made a series directory a candidate would
    /// suppress every volume beneath it, and without this number the result
    /// reads as a library that happens to hold nothing rather than as a rule
    /// that swallowed it.
    pub suppressed: usize,
```

```rust
#[test]
fn a_suppressed_entry_is_counted_rather_than_vanishing() {
    let tree = crate::tests::tempdir("derive-suppressed-count");
    let volume = tree.path.join("Series").join("Series - v01");
    for chapter in ["c001", "c002", "c003"] {
        crate::scan::tests::write_pages(&volume.join(chapter), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.members, 1);
    assert_eq!(report.suppressed, 3, "the three chapter directories");
}
```

Task 10's `library scan` report prints it when it is non-zero.

- [ ] **Step 7e: A volume whose files change stays the same work**

*Found by Task 7's review, measured on a scratch checkout, and settled by the owner on 2026-08-17.*

**The defect.** Add one page to a volume directory and rescan: `scan::record` nulls `sha256` because size and mtime moved, `derive` re-hashes, `identity::content_key` — which hashes the hub edition's member digests — returns a different key, `find_by_key` misses, and `import_with` inserts a **second work**. The reader's progress stays on the first; the library serves the second. Both then sit at the same `(series_id, volume)`, and the hub lookup has no `ORDER BY` or `LIMIT`, so from that point the hub is read off whichever row SQLite happens to return.

Measured: scanning the fixture, deriving, recording progress on volume 1, adding one page and re-deriving gives three `work` rows — two of them at `volume = 1.0`, with `progress` on the orphan. Adding a chapter to a chapter-organised volume does the same.

This is the harm property 1 exists to prevent, arriving through the member set rather than through the hub key, and it is triggered by the most ordinary events in this corpus: a repacked `.cbz`, a corrected scan, a chapter arriving.

**The fix: `(series_id, volume)` is a scanned work's stable address, the way `sidecar_path` is a document-backed one's.** `import` already has exactly this shape — content decides first, and a known address decides on a miss. Give `scanned` the same second probe:

```rust
/// Import a work the **scanner** derived, which has no document on disk.
///
/// `known` is the row derivation already found for this `(series_id,
/// volume)`, or `None` for a work it has never seen. It is the second
/// identity probe, and it is the exact analogue of [`sidecar`]'s
/// `sidecar_path`: content decides first, and a known address decides on a
/// miss.
///
/// **Without it, a volume whose files change becomes a second work.**
/// `content_key` hashes the hub edition's member digests, so adding a page,
/// repacking an archive or fixing a scan changes the key, `find_by_key`
/// misses, and a new row is inserted while the reader's progress, notes and
/// history stay on the old one. Measured on 2026-08-17.
///
/// **What keeps this safe is not that the address is unique.** An earlier
/// version of this comment claimed two genuinely different works cannot
/// share a series and a volume, so the probe could not merge things that
/// should be apart. That is false, and the review that raised this finding
/// disproved it: an address can be **vacated and reoccupied**. Remove volume
/// 1 from a root, renumber volume 2 to take its place, and repack it so
/// content cannot rescue the match, and this probe hands volume 1's row —
/// with a reader's volume-1 progress on it — to what was physically volume
/// 2. Measured.
///
/// What actually makes it right is the **order** and the **counter**:
/// `content_key` decides first, so `known` fires only for content this
/// library has never seen, and `DeriveReport::rekeyed` fires whenever it
/// does. The counterfactual is worse rather than better — without `known`
/// the same scenario mints a third row and strands the progress outright —
/// so this is a trade made deliberately, not an impossibility.
pub fn scanned(conn: &Connection, doc: &Sidecar, known: Option<i64>) -> Result<i64, String>
```

`import_with`'s lookup becomes `find_by_key(&tx, &key)?.or(known)` on the `Scan` path, and `update_work` already writes the new `content_key`, which is what makes the row follow its files.

**Detection stays too, because a reconcile that silently did the wrong thing would be worse than the bug.** `DeriveReport` gains `rekeyed: usize` — works whose `content_key` moved under them — for the same reason `suppressed` exists: a number nobody prints is a change nobody can see.

```rust
#[test]
fn a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first");

    let (id, key_before): (i64, String) = conn
        .query_row(
            "SELECT id, content_key FROM work WHERE volume = 1.0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 7)",
        [id],
    )
    .expect("progress");

    std::fs::write(
        tree.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01 (2019)")
            .join("003.jpg"),
        b"one more page",
    )
    .expect("write");
    scan::scan_root(&conn, &root).expect("rescan");
    let report = derive::derive(&conn).expect("second");

    let at_volume_one: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE volume = 1.0", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(at_volume_one, 1, "one work, not two");

    let (still, key_after): (i64, String) = conn
        .query_row(
            "SELECT id, content_key FROM work WHERE volume = 1.0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("work");
    assert_eq!(still, id, "the same row followed its files");
    assert_ne!(key_after, key_before, "and took the new key with it");
    assert_eq!(report.rekeyed, 1, "reported rather than silent");

    let slot: i64 = conn
        .query_row("SELECT slot FROM progress WHERE work_id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("progress survived");
    assert_eq!(slot, 7);
}
```

**Two smaller things from the same review:**

- **Nothing pins that a second identical derive does not grow the tables.** `DeriveReport`'s doc claims "idempotent by construction" and only `series` is counted. Assert `work`, `edition`, `member` and `series` counts after a second derive in the existing test.
- **A spoke edition's restored page flags can land on shifted pages.** Under `Scope::Scan` flags are restored by `edition.key`, and the work survives whenever the *hub*'s members are unchanged — so a spoke that gains or loses a member keeps its credit and spread flags at their old page numbers. Restoring still beats deleting; the residual case wants a sentence in that doc, since re-running `credits --write` is the only recovery and nothing signals the need.

- [ ] **Step 7c: Two things this task's own steps got wrong about the code**

*Both found by the implementer refusing to make working code match a brief that was wrong about it. Recorded rather than quietly corrected.*

**Step 5 and Step 6 were unnecessary.** They claim `member.chapter_declared` is erased by `replace_editions` and have you add preservation. It was already there, landed in `37602a0` with the library store, keyed on **`sha256`** (`import.rs`'s `held_member_meta`). The brief's proposed `(edition key, ordinal)` keying is also *worse*: a chapter edition that gains a chapter shifts every later position, so position-keying would move every subsequent chapter number onto the wrong file. The claim came from reading `taiyaku_core::Member`'s three fields and inferring the rest without reading `replace_editions`. **Skip Steps 5 and 6; the behaviour they ask for exists and is better than what they specify.**

**Step 7's `ON CONFLICT DO NOTHING` cannot work.** `series.sort_title` has no unique constraint (`001_initial.sql`), so there is no conflict to do nothing about and every derive would insert a duplicate row, silently. `SELECT` then `INSERT` is the correct shape against this schema. A `UNIQUE (sort_title)` in a later migration is the durable fix and belongs to whoever adds migration 4 — it is not free, since an existing database may already hold duplicates.

- [ ] **Step 7: Run it and gate**

Run: `cargo test -p taiyaku-library`
Expected: PASS.

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "Facts and rules become structure, and a rule edit re-derives a library that is no longer on disk"
```

---

## Task 8: Proposals, the scoring vocabulary, and accepting by writing a rule

§6 and §6.1. The judgements §1 measured and found no rule should make: roughly 20 right to 17 wrong on suffix-stripping, which is barely better than a coin flip.

**Files:**
- Create: `crates/taiyaku-library/src/proposals.rs`, `crates/taiyaku-library/src/proposals/tests.rs`
- Modify: `crates/taiyaku-library/src/derive.rs` (raise them at the end of a derive), `crates/taiyaku-library/src/lib.rs`

**Interfaces:**
- Consumes: `derive::derive`'s series and unattached chapter directories
- Produces:
  - `proposals::Proposal { id, kind, subject_id, target_id, score, reason, state, created_rule_id }`
  - `proposals::raise(conn: &Connection) -> Result<usize, String>` — idempotent; returns how many are newly open
  - `proposals::open(conn: &Connection, min_score: f64) -> Result<Vec<Proposal>, String>`
  - `proposals::accept(conn: &Connection, id: i64) -> Result<i64, String>` — returns the `rule.id` it wrote
  - `proposals::unaccept(conn: &Connection, id: i64) -> Result<(), String>`
  - `proposals::dismiss(conn: &Connection, id: i64) -> Result<(), String>`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_dismissed_proposal_stays_dismissed_across_a_rescan() {
    // §6's first argument. Without this, every rescan re-raises the same 37
    // suffixed directories forever and the person stops reading them.
    let conn = library_with_two_suffixed_series();
    proposals::raise(&conn).expect("first");
    let first = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(first.len(), 1);
    proposals::dismiss(&conn, first[0].id).expect("dismiss");

    let newly_open = proposals::raise(&conn).expect("second");

    assert_eq!(newly_open, 0);
    assert!(proposals::open(&conn, 0.0).expect("open").is_empty());
}

#[test]
fn accepting_writes_a_rule_and_un_accepting_removes_it() {
    // §6.1. Accepting must not write structure directly, or derivation would
    // have two inputs and §2's re-derive claim would need a merge rule to
    // protect manual accepts from it.
    let conn = library_with_two_suffixed_series();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");

    let (kind, pattern): (String, String) = conn
        .query_row("SELECT kind, pattern FROM rule WHERE id = ?1", [rule_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("rule");
    assert_eq!(kind, "prefix", "narrow: this exact directory");
    assert!(pattern.contains("Full Color"), "{pattern}");

    proposals::unaccept(&conn, proposal.id).expect("unaccept");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE id = ?1", [rule_id], |r| r.get(0))
        .expect("count");
    assert_eq!(rules, 0, "the rule went with the acceptance");
    assert_eq!(proposals::open(&conn, 0.0).expect("open").len(), 1);
}

#[test]
fn the_vocabulary_ranks_and_never_decides() {
    // §6: `Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)`
    // matches "Color Edition" and is the colour edition of a SPIN-OFF. Even
    // the best signal available gets that one wrong, so a high score is still
    // a proposal.
    let conn = library_with_series(&[
        "Shingeki no Kyojin",
        "Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)",
    ]);
    proposals::raise(&conn).expect("raise");
    let raised = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].state, "open", "proposed, never applied");
    let structure: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| r.get(0))
        .expect("count");
    assert_eq!(structure, 0, "raising writes no rule");
}

#[test]
fn the_measured_false_positives_score_below_the_true_ones() {
    // §1's four named false positives — unrelated proper nouns whose only
    // shared property is scoring low, because the true cases carry a
    // vocabulary match AND an equal volume count and these carry neither.
    // This is what the score threshold has to separate; it is not a claim
    // that the threshold is right, which one library cannot establish.
    let conn = library_with_series(&[
        "Kobayashi-san Chi no Maid Dragon",
        "Kobayashi-san Chi no Maid Dragon - Kanna no Nichijo",
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ]);
    proposals::raise(&conn).expect("raise");
    let raised = proposals::open(&conn, 0.0).expect("open");
    let scored: std::collections::HashMap<String, f64> = raised
        .iter()
        .map(|p| (p.reason.clone(), p.score))
        .collect();
    let colour = scored.iter().find(|(r, _)| r.contains("Full Color")).expect("colour").1;
    let spin_off = scored.iter().find(|(r, _)| r.contains("Kanna")).expect("spin-off").1;
    assert!(colour > spin_off, "{colour} should outrank {spin_off}");
}
```

The two helpers those tests use go at the top of the same file. They insert `series` rows directly rather than scanning and deriving a tree, because what is under test is the scoring and the state machine, not the walk:

```rust
/// A library holding one series per title, **derived from a real scanned
/// tree** rather than from hand-inserted rows.
///
/// **It used to insert `series` rows directly, and that stopped being
/// adequate on 2026-08-17.** Once `accept` derives its rule's pattern from
/// the paths of the entries that resolved to a series — and verifies the
/// rule matches something — a series with no `scan_entry` rows behind it has
/// nothing to accept, and refusing is the correct answer. A fixture that
/// cannot reach `accept`'s success path cannot test it.
///
/// The weaker version also hid a second thing: with no `work` rows anywhere,
/// every volume-count comparison was `0` against `0`, so the scoring tests
/// discriminated on vocabulary alone while appearing to exercise both
/// signals. Building from a scanned tree fixes both at once, which is the
/// argument for doing it here rather than per test.
///
/// Returns the `Library` rather than a bare `Connection`: `Library` owns the
/// connection behind a `Mutex` and there is no `into_connection`, deliberately
/// — widening this crate's public surface so four tests can hold a connection
/// directly is the wrong trade. Each test takes `&library.conn()` itself,
/// which is what every other test module here already does.
fn library_with_series(titles: &[&str]) -> Library {
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        for title in titles {
            conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [title])
                .expect("series");
            let id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
                rusqlite::params![id, title],
            )
            .expect("title");
        }
    }
    library
}

fn library_with_two_suffixed_series() -> Library {
    library_with_series(&[
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ])
}
```

The tests above are written as `let conn = library_with_two_suffixed_series();` and then pass `&conn`. With this helper they read `let library = library_with_two_suffixed_series(); let conn = library.conn();` and pass `&conn` — adjust each of the four as you write it. The `library` binding must outlive `conn`, so bind both rather than chaining.

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library proposals::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write `proposals.rs`**

The vocabulary is §6's list, verbatim, as a constant with the sentence that governs it:

```rust
/// The edition words, and what they are for.
///
/// **The vocabulary ranks; it never decides.** §1's roughly-20-to-17 hand
/// classification is the whole argument, and this list does not rescue it:
/// `Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)` matches
/// "Color Edition" and is the colour edition of a *spin-off*, so even the best
/// signal available gets that one wrong.
const EDITION_WORDS: [&str; 8] = [
    "Fan Translated",
    "Color",
    "Colour",
    "Kanzenban",
    "Perfect Edition",
    "Definitive Edition",
    "Deluxe Edition",
    "Digital Colored Comics",
];
```

`raise` finds each series whose `sort_title` is another series' title plus a ` - ` suffix, scores the pair, and `INSERT OR IGNORE`s an `edition-of` proposal — the expression index from Task 4 is what makes the second call a no-op over a dismissal. Scoring, from §6:

- a vocabulary hit in the suffix, and
- an equal volume count between the two series.

§6 is explicit that the pattern-level control for dismissal is **a score threshold, not a suppression list**, because no pattern covers `- Kanna no Nichijo`, `- Lost Girls`, `- Gaiden`, `- Kurapika's Memories`: they are unrelated proper nouns whose only shared property is scoring low. `open` therefore takes `min_score` and the weak ones stay reachable rather than hidden.

`same-series` proposals come from §5's second use of `series_title`: a directory whose name matches any known title of a series joins that series — which is what relates 「五等分の花嫁」 to `Go-Toubun no Hanayome`, since no pattern can.

`attach-chapters` comes from `derive`'s unattached directories.

**Two things Task 7 discovered that land here, and neither is a defect in this task — they are §7's model meeting the schema.**

**§7's "members and no work" is not implementable as written.** `member.edition_id` is `NOT NULL` (`001_initial.sql`), and `edition.work_id` is too, so a member cannot exist without an edition and an edition cannot exist without a work. There is nowhere to put a member that belongs to no work. Task 7 therefore records the directory and its chapter facts in `scan_entry` — which is where they already are — and produces no rows at all, counting the directory into `unattached_chapter_dirs`. Read §7's sentence as *"a chapter-organised directory produces no work, and stays visibly unattached"*, which is what it was for: keeping 111-volume One Piece from becoming one work with 111 members.

**An accepted `attach-chapters` proposal cannot produce a work either.** §6.1 says accepting writes a narrow `prefix` rule, and a `prefix` rule sets only the `set_*` columns — `edition_key`, `kind`, `language`, `series`. **None of them is a volume**, and `derive` needs a volume to make a work. So accepting one of these today changes nothing that a person can see, which is worse than refusing: it moves the proposal to `accepted`, writes a rule, and produces no work.

That is a real gap and it is the owner's to close, not this task's to paper over. Three shapes are available and they are not equivalent:

1. **`attach-chapters` is not acceptable yet** — raise it, let it be dismissed, and refuse `accept` on that kind with a message saying which command does the job (`taiyaku align raw-v01.cbz -- en-c001.cbz en-c002.cbz …`, which is the README's own example and the thing §7 says only the alignment can answer). Smallest, honest, and loses nothing: the proposal is still the inbox entry that says "these chapters are unattached".
2. **Give `rule` a `set_volume` column** in a later migration, so an accepted proposal can say "this directory is volume 3". Contradicts §4's stated reason for having no such column — "a literal volume number is a property of one file, not of a rule that matches many" — and one rule per volume is a rule per volume.
3. **Let acceptance write structure directly**, which §6.1 exists to forbid: derivation would then have two inputs and re-deriving would either discard the accepts or need a merge rule to protect them.

**Implement shape 1** unless the owner says otherwise. It is the only one that does not either contradict a settled decision or ship an accept that silently does nothing, and it leaves the other two open once someone has actually tried to attach a chapter edition by hand.



`reason` is stored as text and shown, because a proposal whose basis is invisible is a coin flip with a confidence score attached.

**Absence of evidence is not agreement.** The equal-volume-count half of the score compares how many works each series holds — and when either side holds **none**, there is nothing to compare, so it must contribute nothing rather than counting as a match. `0 == 0` is not two series agreeing about their volumes; it is two series about which the question has not been asked. Counting it as agreement inflates the score of exactly the cases with the least evidence behind them, which is backwards.

That matters more than it looks, because the fixtures in this task create no `work` rows at all. With `0 == 0` scoring as equal, every proposal collects the volume point, the discrimination between §1's true and false positives rests on vocabulary alone, and **the volume half of the score is exercised by nothing** — while `the_measured_false_positives_score_below_the_true_ones` still passes, which is the shape of a test that proves less than it appears to.

```rust
#[test]
fn the_volume_count_signal_needs_volumes_to_count() {
    // §6 scores on a vocabulary hit AND an equal volume count. Three cases,
    // and the middle one is the whole point: two series that agree because
    // they each hold three volumes are evidence, and two series that "agree"
    // because neither holds any are not.
    let equal = score_for(&[("Series", 3), ("Series - Full Color Edition", 3)]);
    let unequal = score_for(&[("Series", 3), ("Series - Full Color Edition", 1)]);
    let neither = score_for(&[("Series", 0), ("Series - Full Color Edition", 0)]);

    assert!(equal > unequal, "{equal} should beat {unequal}");
    assert_eq!(
        neither, unequal,
        "no works on either side is no evidence, not agreement — it must not          score as though the counts matched"
    );
}
```

`score_for` inserts each series with that many `work` rows attached, raises, and returns the one proposal's score. Write it beside the other helpers; the existing ones create no works, which is why this gap existed.

**`CERTAIN = 3.0` for `same-series` and `attach-chapters` is a judgement, not a measurement**, and must say so where it is defined. §1 measured `edition-of`'s signal and nothing else; the other two kinds have no survey behind them. A constant that looks like the others but rests on nothing is how an unmeasured number comes to be quoted as if it were measured — this file's own preamble is that every claim traces to a measurement in `notes.md`, and this one does not.



`accept` writes a narrow `prefix` rule matching that exact directory, records its id in `created_rule_id`, and sets `state = 'accepted'`.

**Three things `accept` and `dismiss` must get right, each found by running the shipped code rather than reading it.**

**1. The pattern must come from the paths, not from the title.** An earlier version of this step derived the `prefix` pattern from the subject series' `sort_title`, on the reasoning that seeded rule 1 resolves a series from the first path component so the title *is* that component. **That is false whenever a higher-ordinal rule decides the series** — seeded rule 4, `^(?<series>.+?)[ _-]+v` on `name`, exists precisely because filenames repeat the series. Measured on a root holding `Publisher/Go-Toubun no Hanayome v01.cbz` and its colour sibling: the written rule was `prefix` on `path` with pattern `Go-Toubun no Hanayome - Full Color Edition/`, every path begins `Publisher/`, so it **matched nothing** — the proposal read `accepted` forever and the library was untouched.

That is the "accepted, and nothing a person can see" outcome this module **refuses `attach-chapters` to avoid**, reached by a different road. Derive the pattern from the `rel_path`s of the entries that actually resolved to the subject series — their common directory prefix — and **verify after writing that at least one `scan_entry.rel_path` matches it**. If no single common prefix exists, or nothing matches, refuse by name rather than writing a rule that decides nothing:

```
this proposal's subject spans more than one directory, so accepting it cannot
be expressed as one prefix rule. Its entries are under: <paths>
```

The verification is the load-bearing half: a pattern derived correctly today can still match nothing after a rule edit moves the series, and a rule that decides nothing is indistinguishable from a decision that was never made.

**And it must ask "does this match only the subject", not merely "does this match something".** A first version of this fix asked only the weaker question, and traded a rule that matched nothing for one that matched a whole shared directory. Measured, on the layout this task's own fixture codifies — one publisher directory holding several series:

```
accepted the colour proposal -> rule pattern `Publisher/` set_series="Go-Toubun no Hanayome"
after re-derive:
  series="Go-Toubun no Hanayome" work=3 vol=1 member=…/Chainsaw Man - v01
  series="Go-Toubun no Hanayome" work=3 vol=1 member=…/Go-Toubun no Hanayome - Full Color Edition - v01
  series="Go-Toubun no Hanayome" work=3 vol=1 member=…/Go-Toubun no Hanayome - v01
  series="Go-Toubun no Hanayome" work=2 vol=2 member=…/Chainsaw Man - v02
```

One accept moved **both Chainsaw Man volumes into another series** — v01 as a third member of Go-Toubun volume 1's edition, v02 as Go-Toubun volume 2 — with nothing said to the person, and `derive` afterwards reporting `series: 1` because every file under `Publisher/` now resolves to one series. **The previous bug did nothing; this one mis-attributes other people's material into another work's page sequence, which is strictly worse** — `write_work`'s own comment records that member order is page order and every alignment offset is computed from it.

So after deriving the pattern, refuse if any `scan_entry` matching it resolves to a series **other than** the subject, naming what it would have swallowed:

```
this proposal's directory also holds <series>, so accepting it would move
that material too. Narrow the layout or write the rule by hand.
```

Same shape as the spans-more-than-one-directory refusal, and the same doctrine: refuse rather than write a rule whose effect nobody asked for.

**An `edition-of` accept must set the edition key, not only the series.** Setting `set_series` alone makes the colour volume **extra pages of the base edition** rather than a second edition of the work. Measured on a clean flat layout: after accept and re-derive, three works each with **one** `raw` edition holding two members — so Go-Toubun volume 1's raw edition is the base volume's pages followed by the colour volume's. For a proposal whose entire claim is "this is the colour *edition* of that", that is the wrong outcome and it is visible to a reader.

`set_edition_key` takes the suffix that distinguished the subject in the first place — the part of its `sort_title` after the base title — slugified: `Go-Toubun no Hanayome - Full Color Edition` gives `full-color-edition`. That is the same string the proposal's own `reason` quotes, so the rule says what the proposal said.

**2. `accept` and `dismiss` need state guards, and both holes lose a rule.** Measured:

- **Double `accept` succeeds and orphans a rule.** The second call writes a second rule and overwrites `created_rule_id` with its id, so `unaccept` deletes only the second and the first stays in force — deciding structure, invisible to the proposal that made it, while the proposal reads `open`. That is exactly what `created_rule_id` exists to prevent.
- **`dismiss` over an accepted proposal leaves the rule alive** under a `dismissed` label. The proposal drops out of `open`, `reopen_orphaned_accepts` only looks at `state = 'accepted'` so it never notices, and the rule keeps deciding.

`accept` refuses unless `state = 'open'`. `dismiss` refuses an accepted proposal and says to un-accept first — clearer than silently un-accepting, which would delete a rule from under a command whose name says it only changes a label.

**3. Score and `reason` are frozen at first raise.** `INSERT OR IGNORE` never updates an existing row, so a proposal raised before its series had any works keeps `score = 1` and a reason reading "neither side has any known volumes yet" after both sides gain three volumes — and `open(min_score)` keeps hiding it. Measured. After the insert:

```sql
UPDATE proposal SET score = ?1, reason = ?2
 WHERE kind = ?3 AND subject_id = ?4 AND ifnull(target_id, -1) = ?5
   AND state = 'open'
```

`state = 'open'` is what keeps decision 1 intact — a dismissed or accepted row is never rescored — and the update must not count into `newly_open`, which means "proposals a person has not seen", not "rows touched".

 `unaccept` deletes the rule, nulls the column and sets `state = 'open'`. **Dismissal is not symmetric and writes no rule**: doing nothing to a proposal already has an outcome — the directory stays its own series — so accepting *changes* that outcome while dismissing *confirms* it.

`raise` also reopens any accepted proposal whose `created_rule_id` has become NULL, which is what an `ON DELETE SET NULL` on a rule deleted through C4's editor leaves behind.

- [ ] **Step 4: Call it from `derive`**

At the end of `derive::derive`, after structure is written, call `proposals::raise`. It needs the series rows to exist, which is why it runs last rather than beside them.

- [ ] **Step 5: Run the tests and gate**

Run: `cargo test -p taiyaku-library`
Expected: PASS.

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "Proposals: the judgement a measurement said no rule should make, and an accept that writes a rule"
```

---

## Task 9: Change detection — the probe, the watcher, the interval

§8. Whether inotify works is not predictable from a filesystem type: an Unraid user share is a FUSE overlay (`shfs`), a Docker bind mount and `overlayfs` each behave differently, and NFS delivers nothing for a change another host makes. So it is answered by experiment.

**Files:**
- Create: `crates/taiyaku-library/src/watch.rs`, `crates/taiyaku-library/src/watch/tests.rs`
- Modify: `crates/taiyaku-library/Cargo.toml`, `crates/taiyaku-library/src/lib.rs`

**Interfaces:**
- Produces:
  - `watch::Probe { watching: bool, reason: Option<String> }`
  - `watch::probe(root: &Path, timeout: Duration) -> Probe`
  - `watch::interval_for(probe: &Probe) -> Duration`
  - `watch::report_line(root: &Path, probe: &Probe, dirs: usize, interval: Duration) -> String`

- [ ] **Step 1: Add `notify`**

```toml
# §8 of the scanner spec: the watcher, where the probe says one works.
notify = "8"
```

**Verified, not guessed:** `cargo add notify --dry-run -p taiyaku-library` on 2026-08-17 resolves this to **8.2.0**. Unlike `regex`, this is a genuinely new crate — `notify` is not in `Cargo.lock` today — and it brings a subtree with it, so it is the one real dependency this whole branch adds. CI has no cargo cache, so that cost is paid on every run of the `test` and `clippy` jobs; both have a ten-minute budget. If the build time moves noticeably, say so in the report rather than absorbing it silently.

- [ ] **Step 2: Write the failing tests**

```rust
#[test]
fn a_probe_on_an_ordinary_local_directory_sees_its_own_file() {
    // This machine's /tmp is not NFS, so the probe should succeed here. It is
    // a test of the mechanism, not of any deployment: §13 records that
    // whether it works on Unraid's shfs is unknown and cannot be measured
    // from this workstation.
    let dir = crate::tests::tempdir("probe");
    let probe = probe(&dir.path, Duration::from_secs(2));
    assert!(probe.watching, "{:?}", probe.reason);
}

#[test]
fn the_probe_removes_the_file_it_wrote() {
    // §10: the scanner does not write to the library. The probe is the one
    // exception, and it is only an exception while it cleans up after itself.
    let dir = crate::tests::tempdir("probe");
    probe(&dir.path, Duration::from_secs(2));
    let left: Vec<_> = std::fs::read_dir(&dir.path)
        .expect("read_dir")
        .filter_map(Result::ok)
        .collect();
    assert!(left.is_empty(), "the probe left {} entries", left.len());
}

#[test]
fn a_root_that_cannot_be_written_falls_back_to_the_short_interval() {
    // **`chmod 0555` does not stop root**, which holds `CAP_DAC_OVERRIDE` —
    // and CI's `test` job runs `rust:1-bookworm` with no `user:`, doing
    // root-only work in its own steps. Reproduced under `unshare -Ur`: this
    // test FAILS as root, on a property the code gets right.
    //
    // So it measures whether the restriction actually took, and says so when
    // it did not. `taiyaku-serve/src/tests.rs`'s
    // `one_work_that_cannot_be_forgotten_does_not_stop_the_others` stages the
    // same thing the same way, for the same reason, and this follows it —
    // including printing `NOT MEASURED:` rather than passing quietly, which
    // is what `check-all.sh`'s own summary does when something is skipped.

    let dir = crate::tests::tempdir("probe");
    let mut perms = std::fs::metadata(&dir.path).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
    std::fs::set_permissions(&dir.path, perms).expect("chmod");

    // The guard: if this process can still create a file here, the chmod
    // bought nothing and every assertion below would pass without measuring.
    let sentinel = dir.path.join(".taiyaku-permission-check");
    if std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&sentinel)
        .is_ok()
    {
        let _ = std::fs::remove_file(&sentinel);
        eprintln!(
            "NOT MEASURED: a_root_that_cannot_be_written_falls_back_to_the_short_interval (this process can write to a 0555 directory)"
        );
        return;
    }

    let probe = probe(&dir.path, Duration::from_secs(1));

    assert!(!probe.watching);
    assert!(probe.reason.is_some(), "a failure says why");
    assert_eq!(interval_for(&probe), Duration::from_secs(15 * 60));
}

#[test]
fn the_interval_is_long_where_watching_works_and_short_where_it_does_not() {
    // §8: long where watching works — a safety net for events missed while
    // the server was down; short where it does not, because then it is the
    // only mechanism.
    let watching = Probe { watching: true, reason: None };
    let not = Probe { watching: false, reason: Some("no event arrived".into()) };
    assert_eq!(interval_for(&watching), Duration::from_secs(24 * 60 * 60));
    assert_eq!(interval_for(&not), Duration::from_secs(15 * 60));
}

#[test]
fn the_report_names_what_this_root_can_actually_do() {
    // §8's exact shape. Reported per root, never assumed.
    let watching = Probe { watching: true, reason: None };
    let line = report_line(Path::new("/srv/manga"), &watching, 156, interval_for(&watching));
    assert!(line.contains("/srv/manga"), "{line}");
    assert!(line.contains("watching 156 directories"), "{line}");
    assert!(line.contains("24h"), "{line}");

    let not = Probe { watching: false, reason: Some("no event arrived within 2s".into()) };
    let line = report_line(Path::new("/mnt/remote/manga"), &not, 12, interval_for(&not));
    assert!(line.contains("watch probe failed"), "{line}");
    assert!(line.contains("15m"), "{line}");
}
```

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p taiyaku-library watch::`
Expected: FAIL — the module does not exist.

- [ ] **Step 4: Write `watch.rs`**

```rust
//! Whether this root can be watched, answered by experiment.
//!
//! **Not by filesystem type.** No fstype table can know about an Unraid user
//! share (FUSE `shfs`), a Docker bind mount or `overlayfs`, and a table of
//! special cases would be wrong for the next deployment. So the scanner writes
//! a temporary file into the root, watches for the event, and records whether
//! it arrived — one experiment that is correct for this deployment, for the
//! NFS case other people will have, and for filesystems nobody has considered.
//!
//! **This is the one place the scanner writes to the library tree** (§10). The
//! file is dot-prefixed, so `scan.rs`'s walk skips it even in the window
//! before it is removed, and it is removed on every path out of `probe`
//! including the timeout.
//!
//! **A measurement of this workstation, and not of the server.** The store
//! spec's §10 originally reasoned that a watcher delivers nothing over NFS and
//! that the interval must therefore be short. The NFS observation is correct
//! *of this workstation*, which mounts the library over nfs4; the server will
//! run **on** the NAS, where the same tree is local. The conclusion was about
//! the wrong host, and the probe replaces the prediction entirely.
```

`probe` writes `.taiyaku-watch-probe-<pid>`, starts a `notify` recommended watcher on the root non-recursively, waits up to `timeout` for any event naming that path, removes the file, and answers.

**It must create that file exclusively, never with `fs::write`.** `std::fs::write` follows symlinks: if something has already placed a symlink at `.taiyaku-watch-probe-<pid>`, the probe follows it and **truncates whatever it points at**. The pid is enumerable, and this scanner's whole purpose is to run against a media directory on a home server — a tree that may well be writable by other accounts, and is exactly the kind of place a planted symlink survives unnoticed. This is the one write the scanner makes; following a link would let that single exception write anywhere on the host.

```rust
// **`create_new`, not `fs::write`.** This maps to `O_CREAT | O_EXCL`, which
// does not follow symlinks and fails outright if anything already exists at
// the path. `fs::write` follows a symlink and truncates its target, and the
// pid in the name is guessable — so on a share other accounts can write to,
// the scanner's one and only write becomes a write to any file on the host.
//
// A leftover from a crashed run therefore reports rather than being
// clobbered: the root falls back to the short interval and the message names
// the path, so a person can remove it. Silently overwriting whatever is
// there is what this is avoiding, and a stale probe file is precisely the
// case where "whatever is there" is least predictable.
let probe_file = std::fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(&probe_path);
if let Err(e) = probe_file {
    return Probe {
        watching: false,
        reason: Some(format!(
            "creating the probe file at {}: {e}",
            probe_path.display()
        )),
    };
}
```

Removal stays `remove_file`, which unlinks the name rather than following it, so the cleanup path was never the exposed half.

```rust
#[test]
fn a_symlink_in_the_probes_place_is_refused_rather_than_followed() {
    // `fs::write` would follow this and truncate the target. The pid is
    // guessable and a media share may be writable by other accounts, so the
    // scanner's one write must not be a write to an arbitrary file.
    let dir = crate::tests::tempdir("probe-symlink");
    let victim = dir.path.join("important.json");
    std::fs::write(&victim, b"{\"keep\": true}").expect("write");
    let planted = dir
        .path
        .join(format!(".taiyaku-watch-probe-{}", std::process::id()));
    std::os::unix::fs::symlink(&victim, &planted).expect("symlink");

    let answer = probe(&dir.path, Duration::from_millis(200));

    assert!(!answer.watching);
    assert_eq!(
        std::fs::read(&victim).expect("victim survives"),
        b"{\"keep\": true}",
        "the probe followed a symlink and truncated its target"
    );
}
```
 A write that fails — a read-only root — is `watching: false` with the OS error as the reason rather than an error return, because a root that cannot be probed is a root that gets the short interval, not a scan that refuses.

Interval constants carry §8's reasoning:

```rust
/// Long where watching works: a safety net for events missed while the server
/// was down, not the mechanism.
const WATCHED_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Short where it does not, because then it is the only mechanism.
const UNWATCHED_INTERVAL: Duration = Duration::from_secs(15 * 60);
```

**The watch-count ceiling is reported, never silently degraded.** A recursive watch costs one inotify watch per directory. This machine allows 524,288 watches and 1,024 instances, measured 2026-08-14, so it is not a constraint here — but a lower ceiling elsewhere must be reported, because a partially-watched root is the worst of the three states: it looks like the first and behaves like neither. Read `/proc/sys/fs/inotify/max_user_watches` where it exists, compare it to the directory count, and put the shortfall in the report line.

- [ ] **Step 5: Run the tests and gate**

Run: `cargo test -p taiyaku-library watch::`
Expected: PASS. If `a_probe_on_an_ordinary_local_directory_sees_its_own_file` fails, do **not** raise the timeout until you have checked what `/tmp` is on this machine — the test failing is itself a measurement.

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add -A
git commit -m "Whether a root can be watched is an experiment, and the answer is reported rather than assumed"
```

---

## Task 10: `taiyaku library scan` and `rules`, and every measurement this invalidates

§11's step 7 and §12. The commands, the per-root report, and the documents that now say something false.

**Files:**
- Modify: `crates/taiyaku-cli/src/library.rs`, `crates/taiyaku-cli/src/library/tests.rs`
- Modify: `README.md`, `docs/status.md`
- Modify: `scripts/coverage-floor.mjs`, `scripts/status-test-count.mjs` — by **re-running them**, not by editing the numbers by hand

**Interfaces:**
- Consumes: everything above

- [ ] **Step 1: Write the failing test for the command surface**

**Match the pattern already in `crates/taiyaku-cli/src/library/tests.rs`**, which calls `command(&[...])` directly and asserts on the refusal text — it captures no stdout, and adding a capture harness for this task would be a second testing style in one file. Anything worth asserting about *output* is asserted on the pure function that produces the string, in `taiyaku-library`, where Task 9 already tests `report_line`. `scratch(name)` is that module's existing temp-directory helper; reuse it.

```rust
#[test]
fn scan_without_a_root_says_how_to_add_one_rather_than_doing_nothing() {
    let dir = scratch("scan-no-roots");
    let db = dir.join("library.db");
    let message = command(&[
        "scan".into(),
        "--db".into(),
        db.display().to_string(),
    ])
    .expect_err("no roots is a refusal, not a silent success");
    assert!(message.contains("--add"), "{message}");
}

#[test]
fn scan_refuses_an_unknown_flag() {
    let dir = scratch("scan-bad-flag");
    let db = dir.join("library.db");
    let message = command(&[
        "scan".into(),
        "--recursive".into(),
        "--db".into(),
        db.display().to_string(),
    ])
    .expect_err("an unknown flag is a refusal");
    assert!(message.contains("--recursive"), "{message}");
}

#[test]
fn scan_adds_a_root_and_the_second_run_finds_it_without_add() {
    let dir = scratch("scan-add");
    let tree = dir.join("library");
    std::fs::create_dir_all(tree.join("Series/Series - v01")).expect("mkdir");
    std::fs::write(tree.join("Series/Series - v01/000.jpg"), b"page").expect("write");
    let db = dir.join("library.db");

    command(&[
        "scan".into(),
        "--add".into(),
        tree.display().to_string(),
        "--db".into(),
        db.display().to_string(),
    ])
    .expect("add and scan");
    command(&["scan".into(), "--db".into(), db.display().to_string()]).expect("rescan");

    let library = taiyaku_library::Library::open(&db).expect("open");
    let conn = library.conn();
    let roots: i64 = conn
        .query_row("SELECT count(*) FROM root", [], |r| r.get(0))
        .expect("count");
    assert_eq!(roots, 1, "--add is idempotent about the root it names");
}
```

And the round trip, tested in `taiyaku-library` where the functions live rather than through the CLI:

```rust
#[test]
fn rules_export_and_import_round_trip() {
    // §4: rules export and import as JSON, so a layout scheme can be shared
    // or moved between hosts. They live in the database and not in a config
    // file, which would be a second source of truth a UI writing rules would
    // have to keep in step.
    let first = Library::open_in_memory().expect("open");
    let json = rules::export_json(&first.conn()).expect("export");

    let second = Library::open_in_memory().expect("open");
    rules::import_json(&second.conn(), &json).expect("import");

    assert_eq!(rules::export_json(&second.conn()).expect("export"), json);
}
```

`rules::export_json` and `rules::import_json` are two more functions on Task 6's module; add them there if Task 6 is still open when you reach this, or here if it has already landed. `import_json` **replaces** the rule set rather than appending to it — an import that doubled the four seeded defaults would layer each one over itself, which is harmless today and is exactly the kind of thing that stops being harmless once someone edits a default.

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p taiyaku-cli library::`
Expected: FAIL — `scan` is not a subcommand.

- [ ] **Step 3: Add the subcommands**

`crates/taiyaku-cli/src/library.rs`'s `command` already matches on `args.first()` for `import` and `export`. Add `scan` and `rules` beside them:

```
taiyaku library scan  [--add PATH] [--root PATH] [--no-derive] [--db PATH]
taiyaku library rules [--export | --import FILE] [--db PATH]
```

`scan` with no roots refuses and names `--add`, rather than succeeding silently over nothing. `scan` walks each root, derives, raises proposals, and prints §8's report per root plus a summary of what derivation produced. `--no-derive` walks and stops, which is what makes "what would the facts be" answerable without changing structure.

Each root's line is `watch::report_line`. A refused root — §8.1 — prints its refusal and the run continues to the other roots; a root that cannot be scanned must not take the others down with it.

- [ ] **Step 4: Run the CLI tests**

Run: `cargo test -p taiyaku-cli library::`
Expected: PASS.

- [ ] **Step 5: Update `README.md`**

Two places, both named in §12:

- The synopsis block gains `taiyaku library scan` and `taiyaku library rules`. **Check the count word above it** — the block is introduced by a number of commands, and that number is part of the contract, not decoration.
- "Using it" — finding material stops being the person's job. Rewrite the paragraph that tells someone to point `align` at files by hand so it names `library scan` first.

- [ ] **Step 6: Update `docs/status.md`, and run the sweep on the way in as well as out**

```sh
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md        # the count word wraps across the line break
```

On 2026-08-17 that read 44 rows, 33 closed, eleven open, and the document agreed with itself. What this branch changes:

1. **The test count, which appears twice** — in "Five crates, N tests" and again two lines later in "Nine of the N are standing checks". Take N from `scripts/status-test-count.mjs`, not from arithmetic.
2. **The dependency list**, which gains `regex` and `notify`.
3. **The `taiyaku-library` row in the crate table**, which says the schema "carries `root` (spec B's scanner, not built)" and "`series_id`/`volume` on `work`: columns nothing populates yet". Both stop being true. Correct them in place; do not delete the sentence, since what was deferred and why is the point of that row.
4. **The commands synopsis**, per Step 5's README change — the two documents state the same contract and drift apart if only one is edited.
5. **`docs/notes.md` gets the measurements**, not `status.md`: status.md's own preamble promises every claim there traces to a measurement recorded in notes. What a scan of the real library costs — the walk, the sniff, the first hash, wall clock and how many entries — goes there, under a heading naming what was run against what.

**Do not open an item for anything §13 says could not be measured.** Those are named limitations of a shipped thing, not defects: what a scan costs on the deployment host, whether the probe works on Unraid's `shfs`, and whether the default rules are good on anyone else's library. If the branch produces a real defect, open it with the full five-part edit — the count word at the head of the section, the "leaving N" sentence, a table row, a prose bullet, and the "What remains" line.

- [ ] **Step 6b: Nine tasks of findings that also land here**

*Added 2026-08-17. §12 was written before the branch ran; these are what the branch itself produced.*

**Two report fields must print, or they are counters nobody reads.** `DeriveReport` gained `suppressed` (entries an ancestor candidate suppressed) and `rekeyed` (works whose `content_key` moved under them). Both exist because a silent drop is indistinguishable from an empty library and a silent re-key is indistinguishable from growth. Print each when non-zero; a counter that never reaches a person is the failure they were added to prevent.

**`docs/status.md`'s crate table says two things that stop being true.** It describes the schema as carrying `root` for "spec B's scanner, not built", and `series_id`/`volume` on `work` as "columns nothing populates yet". Correct them in place rather than deleting the sentences — what was deferred and why is the point of that row.

**The measurements go in `docs/notes.md`, under a heading naming what was run against what.** `status.md`'s own preamble promises every claim there traces to a measurement recorded in notes. Three belong there:

- **The kanji counter survey**, which decided a marker list: 話 577, 巻 498, and 章, 回 and 卷 all **zero** across the development corpus on 2026-08-17. `章` ships anyway, on knowledge of the language rather than on evidence, and the note must say so — it is the difference between a measured constant and a plausible one.
- **`notify`'s build cost**: 262 against 248 CPU-seconds on a clean full-workspace build, 32 cores. State the measurement and **stop before the CI inference** — this workstation parallelises differently from a cacheless runner, and the branch already corrected one attempt to draw that conclusion.
- **What a scan costs here**, with the deployment host named as unmeasured, per §13.

**§13's four unmeasurables are limitations of a shipped thing, not open items.** Do not add them to the Open items table. Scan cost on the deployment host, the watcher probe on Unraid's `shfs`, the 20-to-17 hand classification, and whether the default rules suit anyone else's library — each is stated where the feature is described.

**Two repo-level traps this branch found, which neither §12 nor the spec anticipated.** Both are places the tooling cannot see what it did, so both will recur:

- **`cargo fmt` manufactures whitespace runs.** Merging a short `\`-continued string bakes the stripped leading indentation back in as literal spaces, and `cargo fmt` does not look inside string literals — so nothing in the gate can catch what the formatter itself introduced. Measured on five strings here, three of them messages a person reads.
- **`NOT MEASURED:` is invisible without a nextest override.** A test that guards itself and skips prints nothing without `--nocapture`, which `check-all.sh` passes only to the corpus runs. A skip nobody sees is indistinguishable from a pass, and this affects the existing `taiyaku-serve` precedent too.

Name both in `docs/ci.md`, which is where this project records how the gate behaves, so the next branch does not rediscover them.

- [ ] **Step 6c: Five things the review found, three of them documents asserting what is false**

**1. The binary's own usage never learned the two new commands.** `crates/taiyaku-cli/src/main.rs:189-190` lists `library import` and `library export` only, under a doc comment promising "every command's shape, one line each". A person running bare `taiyaku` cannot discover `scan` or `rules` while `README.md` and `status.md` both say they exist. Step 6 names two statements of that contract; this is a third, and **no test references `print_usage`**, so nothing could have caught it.

**2. `docs/status.md` asserts a fact it calls unknown, in the same sentence.** It reads "…whether the probe actually works on Unraid's `shfs`, which is unknown by construction: **the experiment answers correctly there**, it has just never been run there." The middle clause states a measured-sounding conclusion about a host nobody has run on. The intended sense is conditional — the experiment is *designed* to answer correctly whatever `shfs` does — and §13 of the spec says exactly that: "it is designed to give a correct answer there, not proven to." Use the spec's own wording.

**3. `docs/notes.md` states a distinction that does not hold.** It claims `章` survives its zero corpus count by a property "`回` and `卷` do not share", namely that it can only fire on an explicit `第N章`. **`回` shares it exactly** — as `Marker { before: "第", after: Some("回") }` it would fire only on `第N回`, structurally identical. The claim is defensible for `卷`, which is a *volume* counter and would mis-read `第7卷` as a chapter. `naming.rs` states it correctly without the comparison; the note's embellishment is what makes it false. Drop `回` from that sentence or drop the comparison.

**4. `README.md` claims a capability that does not exist.** "…and every path below can then be found by name instead of typed out by hand." No command below that line accepts a name — `align`, `verify`, `credits`, `regions` and `score` all take member paths, and the only slug-taking command in the binary is `library export <work>`, which writes sidecars rather than resolving paths. A reader acts on that sentence and gets an error.

**5. `scan` exits 0 even when every root failed**, which is the no-roots refusal's own failure arriving by another door: a scan whose only root is an unmounted share succeeds silently over nothing. **This file already resolved the identical question the other way and wrote down why** — `export_targets` exits non-zero with a final line saying how many of how many failed, "so a script sees it". Follow that precedent, and say so in `status.md`, which currently describes continue-and-name without mentioning the exit status.

- [ ] **Step 7: Re-run the derived measurements rather than editing them**

```bash
node scripts/coverage-floor.mjs
node scripts/status-test-count.mjs
```

Both are named in §12 as measurements this invalidates. They derive their numbers; a hand-edited figure is the drift they exist to prevent.

- [ ] **Step 8: The full gate, both halves**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

The second is the corpus job on its own, about 85 seconds. Never set `TAIYAKU_CORPUS` for the first.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "taiyaku library scan: two commands, a per-root report, and the documents that had said it was not built"
```

---

## Self-review

Run this against the spec before opening the PR.

**Spec coverage.** §2 facts-and-rules → Tasks 5, 6, 7. §3 fact layer → Tasks 2, 4, 5. §4 rule layer and the four defaults → Tasks 1, 4, 6. §5 what a rule cannot do → Task 8 (`same-series` off `series_title`). §6 proposals and §6.1 accept-writes-a-rule → Task 8. §7 works, editions and the chapter case → Task 7. §8 change detection → Task 9; §8.1 the empty-root refusal → Task 5. §9 auto-align → nothing to build; `root.auto_align` is already absent from migration 1, so §9 is satisfied by verification rather than by work — check it in Task 4 and say so in the PR. §10 what is deliberately not done → the constraint at the head of this plan and the doc comments named in Tasks 5 and 9. §11 branch shape → Tasks 4–10, in order, with Tasks 1–3 ahead of them because they are the prerequisites the spec did not know it had. §12 measurements → Task 10.

**One spec section is deliberately not implemented.** §4's per-field provenance *display* is spec C4's; this plan produces the provenance (Task 6) and shows it nowhere. That is the seam the spec drew, not a gap.

**Type consistency to check by hand before starting Task 5:** `Format::token` returns `Option<&'static str>` and `scan_entry.format` is nullable — the `None` case is `Unknown` and must round-trip as NULL, not as the string `"unknown"`. `volume` and `chapter` are `f64` everywhere (`Resolved`, `work.volume`, `member.chapter_declared`), never `u32`, even though `volume_number` returns `Option<u32>` — the conversion happens once, in `rules.rs`, and `f64::from` is the call.

**The riskiest task is 7**, and it is risky in a way a test can catch: if `derive` opens a file for any reason, `editing_a_rule_re_derives_the_library_without_touching_the_filesystem` fails, because the tree is gone. Do not weaken that test by keeping the tree alive. It is the only way to test a negative.

## Execution handoff

Two execution options:

1. **Subagent-driven (recommended)** — a fresh subagent per task, review between tasks.
2. **Inline** — batch execution in-session with checkpoints.


---

## Task 11: The four Criticals the whole-branch review found

*Added 2026-08-18. Ten task reviews passed; the whole-branch review found four Criticals at the one seam no task owned — derivation over a real filesystem shape. All four share a shape: **derivation merges or drops silently, and the report counts groups rather than rows.***

**Why every task review missed them, which decides how they must be fixed.** Every derive fixture builds members out of *directories of page images*, so `pages = 0` never occurs in any test, and no fixture has a container that resolves neither volume nor chapter. The reviewer applied C1's two-line fix and **all 254 library tests still passed**. So each fix below lands with the fixture that would have caught it, and a fix arriving without one is not done.

### C1 — an archive inside a volume directory derives to nothing

Confirmed independently against the shipped binary on `Series/v01/Series v01.cbz`:

```
derived 1 series, 2 works, 2 editions, 2 members, 2 suppressed
work rows: 1 | v02 | volume 2.0 | member pages 0 | …/lib/Series/v02
```

A directory holding archives rather than loose page images gets `pages = Some(0)` from the walk. `builtin-volume` reads `v01` from its **name**, so it becomes a member candidate and suppresses the `.cbz` inside it. `classify` has no zero guard, so the container becomes the member; `digest` hashes a page-less directory, which is `member_digest`'s own "digest of nothing" — **the same hash for every such directory** — so `content_key` collides and every group lands on one row.

**Fix:** require `pages > 0` in `derive::is_candidate`, and `entry.pages.filter(|p| *p > 0)` in `classify`.

**Fixture that would have caught it:** a derive fixture whose members are **archives inside volume directories**, not directories of page images. `taiyaku-library` has no `zip` dependency and must not gain one, so write the archives from bytes — a stored-method zip is a fixed header, the entry names, and a central directory; or have the fixture use `taiyaku-read`'s test corpus shape. If neither is workable, say so and report rather than shipping the fix untested.

### C2 — a scan overwrites the work block a document owns

`update_work` writes `title`, `reading` and `hub` unconditionally; only `sidecar_path` is `COALESCE`d. `replace_editions` deletes every edition the incoming document does not name, under `Scope::Scan` too. Measured across three real commands: `library import` then `library scan` then `library export --in-place` rewrote the reader's own `taiyaku.json` from `"Go-Toubun no Hanayome, volume 1" / ltr / "Japanese raw"` to `"Series - v01" / rtl / "raw"`.

**`import::scanned`'s own doc enumerates what a scan speaks for and names exactly two exemptions.** The work block is not among them.

**Settled:**

1. **Under `Scope::Scan`, `title`, `reading` and `hub` are `COALESCE`-style: a row that already has them keeps them.** A scan derives a title from a filename and a direction from a constant; a document carries what a person or the aligner established. The scan is not entitled to overwrite either.
2. **Under `Scope::Scan`, an edition the incoming document does not name is left alone, not deleted.** "The scan never saw this edition" and "a rule edit removed it" are different facts and the code cannot tell them apart, so it must not act as though it can — deleting a hand-aligned edition is unrecoverable, while an orphaned edition is visible and fixable. **The cost, stated:** a rule edit that renames an edition key leaves the old edition in place. That is why it gets a counter — `DeriveReport.kept_editions`, editions left standing that this derive did not derive — so the orphan is reported rather than discovered.
3. **`edition.label` is carried across a replace**, alongside `language` and `kind`.

**Why the task reviews could not see it:** `import/tests.rs`'s `as_scanned()` helper is documented as "Exactly what `derive.rs` builds", but it clones the document and clears only `alignments`/`spreads`/`credits` — keeping the document's own `work.title`, `work.reading` and edition labels, which are exactly the fields derive does not preserve. **Every `Scope::Scan` test is blind to the overwrite by construction.** Fix the helper to build what `derive` actually builds, and expect existing tests to move.

### C3 — a volume-less container is admitted as a member and not suppressed

`Series/Extras/Art Book` derives one work with **two** members — `Series/Extras` at 5 pages and `Series/Extras/Art Book` at 5 pages. Ten pages for five. `report.suppressed = 0`. Three levels give three members.

`nested_in_candidate` only suppresses under an ancestor that resolved a volume or a chapter, and a volume-less container never does — while `classify` still admits it.

**Fix: suppression is computed from entries that actually became members, not from candidacy.** `title_of` also needs an arm for `rel_path == ""`; the root becoming a member is reachable through any broad `set_series` rule — which is what `proposals::accept` writes — and was measured producing a work with an empty title.

### C4 — every volume-less entry in a series merges into one work

`derive.rs`'s group key is `(series, volume)`, so `ONE PIECE/001..004` — bare numbers, which `volume_number_marked` deliberately refuses — becomes **one** work with four members and 18 pages in one sequence, reported as one work with nothing signalling the merge. `Series/Omake` + `Series/Bonus` merge the same way.

**Settled: entries with neither a volume nor a chapter are keyed individually, by `rel_path`, not merged.** Merging distinct material into one page sequence is the failure `write_work`'s own doc calls out — member order is page order and every alignment offset derives from it. Four separate works with `volume = NULL` is an honest answer a person can correct; one work with 18 concatenated pages is not.

Entries that carry chapters still group, which is §7's chapter case and stays.

**And the merge that remains must be reported.** Where two entries do join one `(series, volume)` edition, count it — `DeriveReport.merged` — for the same reason `suppressed` and `rekeyed` exist.

### The counter principle, stated once

`DeriveReport` was built with `suppressed` and `rekeyed` **so that silent outcomes could not happen**, and the two most consequential silent outcomes on this branch were not among its fields. Any derivation outcome that changes what a person sees and cannot be inferred from the other counts gets a field. That now includes `kept_editions` and `merged`.


---

## Task 12: the whole-branch review's nine Importants

*Added 2026-08-18, after Task 11 closed the four Criticals.*

**I1 — `library scan` always prints "0 proposal(s) raised".** `derive` already calls `proposals::raise` and discards the count; the CLI calls it a second time and every `INSERT OR IGNORE` conflicts, so the printed number is unconditionally zero. Measured with one genuinely open proposal.

This is the failure `derive_summary_prints_suppressed_and_rekeyed_when_nonzero` justifies itself by preventing. **It also invalidates a conclusion already in `docs/notes.md`** — "0 proposals — this corpus's naming does not happen to trip `edition-of`/`same-series`/`attach-chapters`" was read off an instrument that reads zero unconditionally, so the observation cannot support it. Fix the code, then **correct that note**: either re-measure and state the real number, or say the earlier reading was an artefact.

Have `derive` return the count and stop raising in the CLI. One raiser, like one writer.

**I2 — no test asserts anything `library scan` prints.** The only assertions are against the pure formatters with hand-built inputs. That is why I1 shipped. Add a test that runs the command and asserts on its output — `crates/taiyaku-cli/tests/cli.rs` already runs the real binary for `print_usage` and is the right home.

**I3 — nothing can reach a proposal.** `proposals::open`, `accept`, `unaccept` and `dismiss` have no caller outside their own tests; `taiyaku library` has only `import|export|scan|rules`. **Two rounds of Critical fixes went into an `accept` a person cannot invoke.**

Add `taiyaku library proposals [--min-score N] [--accept ID] [--dismiss ID] [--unaccept ID] [--db PATH]`, listing by default. The functions exist and are tested; this is the surface over them. Keep the refusals visible — `accept` on an `attach-chapters` proposal names `taiyaku align`, and the swallow-check names what it would have taken.

**I4 — one unreadable directory aborts the scan of its whole root.** `read_dir` fails, `measure_dir` returns `Err`, the root fails. The module's stated principle is that one unreadable *file* must not leave a library unscanned; it was never applied to directories. **On the deployment shape this exists for, a mount point, `lost+found` is 0700 root** — so a non-root `taiyaku library scan --add /srv/manga` fails outright.

Skip a directory that cannot be listed, count it, and name it in the report. `ScanReport` gains `unreadable_dirs`. `Err` from the walk is then reserved for the root itself.

**I5 — `accept` is not transactional.** `INSERT INTO rule`, verify, `UPDATE proposal`. A failure or a crash between leaves a live rule no proposal points at while the proposal still reads `open` — and a second accept then writes a second rule and records only that one, which is the state `created_rule_id` exists to prevent, reached by another road. Wrap all three in one `unchecked_transaction`.

**I6 — an accepted rule is written with `root_id` NULL, so it applies to every root.** Meanwhile `other_series_under_prefix` and the verification scan every root's `scan_entry`. On a library with two roots sharing a directory name, an accept silently re-series the second root's material. Scope the rule to the subject's root.

**I7 — a hand-set `work.volume`/`series_id` is reverted by the next derive, and a hand-renamed series is duplicated.** `derive` writes both unconditionally, while `librarydoc::apply` is the documented mechanism for setting them and `<stem>.library.json` exists so hand metadata survives a re-derivation. Measured: `volume = 4.5` → `1.0` after one derive; a renamed series mints a second row and moves the works to it, leaving the renamed one empty.

**Settled, narrowly:** derive does not overwrite `volume` or `series_id` on a work whose `migrated_document` carries a `library` row — that row is the stored fact that a hand document was applied, and it is already what `migrate` uses to decide the same class of question. Everything else about hand-versus-derived metadata is a real design question and **belongs to spec C3, not to this branch**; record it as such rather than inventing a provenance column here.

**I8 — `MatchOn::Parent` is not distinguished from `MatchOn::Path` by any test**, and `Engine::compile`'s sort by `(ordinal, id)` — the whole layering model — is pinned by nothing. Both are in the mutation-survivor list. Fix with the survivors in Task 13; noted here so the two lists do not disagree.

**I9 — there is no standing corpus check for the scanner.** The corpus job runs 21 checks and none touches `scan`, `derive` or `rules`. **The only real-data evidence for the scanner is an ad-hoc timing run in `notes.md` — which is where both C1 and I1 were misread as expected behaviour.**

Add a gated check that scans a corpus tree and asserts structure: works derived, members non-zero-paged, and that no work holds members from two different series directories. The review's own words: a standing check over `corpus/` would have caught C1 in one assertion. Follow `crates/taiyaku-library/src/corpus.rs`'s existing shape and skip cleanly without `TAIYAKU_CORPUS`.

# Duplicate resolution implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `taiyaku inventory` and `taiyaku duplicates`, turning the
duplicate-resolution protocol of `docs/status.md` §2 from a shell session into
two commands.

**Architecture:** `inventory` walks a root and groups directories that claim to
be the same volume, signing each copy from a directory listing so the sets that
need no transfer are visible before anything is copied. `duplicates` takes the
copies of one volume, settles signature-identical ones with a content hash,
solves every remaining pair, collapses identity clusters, flags copies that are
not the volume at all, and ranks what is left. Both print a table for a person
and one fixed-field line for a survey script.

**Tech Stack:** Rust 2024 edition, rust-version 1.85. `taiyaku-core` for
`solve`/`verdict`/`paired_slots`, `taiyaku-read` for page listing, `sha2` for
digests, `zip` transitively.

**Design:** [`../specs/2026-08-01-duplicate-resolution-design.md`](../specs/2026-08-01-duplicate-resolution-design.md)

## Global Constraints

- **No new external dependency.** Task 2 adds `sha2 = "0.11.0"` to
  `crates/taiyaku-cli/Cargo.toml`. That is a new *edge*, not a new dependency:
  `sha2` is already in the tree via `taiyaku-read` and already named in
  `docs/status.md`'s dependency list, so the documented set does not change. Do
  not add `regex` or anything else.
- **CI runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`.**
  A warning fails the build.
- **`missing_debug_implementations = "warn"`** is set workspace-wide. Every
  public type below derives `Debug`.
- **Neither command moves or deletes anything, and writes nothing but the
  shared fingerprint cache.** `duplicates` fills `.taiyaku-cache` through
  `cached_edition` exactly as `align` does; that is the only write either
  command makes. Nothing in the library is touched, and no report is written to
  disk.
- **Fixed-field lines go to stdout; tables for a person go to stderr.** The
  `set:` and `inventory:` lines never wrap and their field order never changes,
  because `scripts/margin-survey.sh` already establishes that a survey collects
  these with `grep` and reads down a column.
- **Tests that touch the filesystem use `std::env::temp_dir()` and clean up
  after themselves**, following `crates/taiyaku-read/src/pages.rs`'s test
  module. Corpus tests are gated on `TAIYAKU_CORPUS` and print a skip line when
  it is unset.
- Run `cargo test --workspace` after every task. Run
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` before
  every commit.

---

## File structure

| File | Responsibility |
| ---- | -------------- |
| `crates/taiyaku-read/src/pages.rs` | gains `member_page_sizes` and `is_page_name` — both are facts about which entries are pages and how big they are, which is this crate's job |
| `crates/taiyaku-read/src/lib.rs` | re-exports the two |
| `crates/taiyaku-cli/src/signature.rs` | `Signature`, its digest, and the page-bytes digest that confirms a match. Shared by both commands |
| `crates/taiyaku-cli/src/duplicates.rs` | the comparison core (identity, clustering, flagging, ranking), the two output forms, argument parsing, and the filesystem wiring |
| `crates/taiyaku-cli/src/inventory.rs` | the walk, the candidate rule, the volume-number parse, `--pattern`, and its table |
| `crates/taiyaku-cli/src/main.rs` | two match arms and two usage lines |
| `crates/taiyaku-cli/src/corpus.rs` | five gated checks |
| `scripts/duplicate-survey.sh` | collects `set:` lines across a directory of sets |

---

### Task 1: `member_page_sizes` and `is_page_name` in `taiyaku-read`

**Files:**
- Modify: `crates/taiyaku-read/src/pages.rs`
- Modify: `crates/taiyaku-read/src/lib.rs:13-16`
- Test: `crates/taiyaku-read/src/pages.rs` (the existing `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `taiyaku_read::member_page_sizes(path: &Path) -> Result<Vec<u64>, String>`
  - `taiyaku_read::is_page_name(name: &str) -> bool`

**Why here.** The signature is over page entries and their *uncompressed* sizes,
so that a directory of loose images and a `.cbz` of the same scan sign
identically. Which entries are pages is normative — `docs/sidecar.md`, "How a
member becomes pages" — so a second copy of the extension list in the CLI would
be exactly the duplication O10 was closed to remove.

- [ ] **Step 1: Write the failing tests**

Add to the bottom of `mod tests` in `crates/taiyaku-read/src/pages.rs`, above
the closing brace. The `cbz` helper already exists in that module at line ~421.

```rust
    /// The signature a duplicate set is settled by is a list of page sizes, so
    /// this has to agree with `read_edition` on *which* entries are pages, on
    /// what order they come in, and on how big each one is. Uncompressed, not
    /// as stored: an archive and a directory holding one scan must produce the
    /// same list, which is the whole point of signing pages rather than files.
    #[test]
    fn page_sizes_are_uncompressed_in_reading_order_and_skip_non_pages() {
        let path = cbz(
            "taiyaku-test-sizes.cbz",
            &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
        );

        let pages = read_member(&path).unwrap();
        let sizes = member_page_sizes(&path).unwrap();

        let _ = std::fs::remove_file(&path);
        let expected: Vec<u64> = pages.iter().map(|p| p.bytes.len() as u64).collect();
        assert_eq!(sizes, expected, "sizes must match the bytes actually read");
        assert_eq!(sizes.len(), 3, "ComicInfo.xml is not a page");
    }

    /// The property the duplicate signature rests on: repackaging a scan from
    /// loose images into a cbz must not change what it signs as.
    #[test]
    fn a_directory_and_an_archive_of_the_same_pages_size_identically() {
        let dir = std::env::temp_dir().join("taiyaku-test-sizes-dir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
            std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
        }
        // Stored in a different order, as a repacker would.
        let archive = cbz("taiyaku-test-sizes-arc.cbz", &["p10.jpg", "p2.jpg", "p1.jpg"]);

        let from_dir = member_page_sizes(&dir).unwrap();
        let from_archive = member_page_sizes(&archive).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&archive);
        assert_eq!(from_dir, from_archive);
        assert_eq!(from_dir, vec![6, 6, 7], "p1.jpg, p2.jpg, p10.jpg by length");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-read page_sizes`
Expected: FAIL — `cannot find function 'member_page_sizes' in this scope`.

- [ ] **Step 3: Implement**

In `crates/taiyaku-read/src/pages.rs`, rename the private `is_image` to a public
`is_page_name` and keep its body. Replace lines 28-34:

```rust
/// Whether an entry name is a page.
///
/// Public because the duplicate inventory counts pages without reading them and
/// must not carry its own copy of this list — which entries are pages is
/// normative (`docs/sidecar.md`, "How a member becomes pages"), and a second
/// list is the duplication O10 was closed to remove.
pub fn is_page_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
```

Then replace every remaining `is_image(` call in the file with `is_page_name(`.
There are three: in `directory_page_paths`, in `page_names_from`, and in the
`skipped` diagnostic path if present — find them with
`grep -n 'is_image' crates/taiyaku-read/src/pages.rs`.

Add after `member_pages` (around line 353):

```rust
/// The uncompressed byte size of each of a member's pages, in reading order,
/// without reading any of them.
///
/// Directory pages are sized from their metadata and archive entries from the
/// zip central directory, which records the uncompressed size. Uncompressed on
/// both sides deliberately: a directory of loose images and a `.cbz` of the
/// same scan then produce the same list, and seeing through packaging is the
/// whole job of the signature this feeds.
///
/// A listing, not a read — the property that lets an inventory run over a NAS
/// and decide what is worth copying.
pub fn member_page_sizes(path: &Path) -> Result<Vec<u64>, String> {
    if path.is_dir() {
        return directory_page_paths(path)?
            .iter()
            .map(|page| {
                std::fs::metadata(page)
                    .map(|meta| meta.len())
                    .map_err(|e| format!("{}: {e}", page.display()))
            })
            .collect();
    }
    let mut zip = open_archive(path)?;
    let names = page_names_from(&mut zip);
    names
        .iter()
        .map(|name| {
            zip.by_name(name)
                .map(|entry| entry.size())
                .map_err(|e| format!("{}: {name}: {e}", path.display()))
        })
        .collect()
}
```

- [ ] **Step 4: Export both**

In `crates/taiyaku-read/src/lib.rs`, replace the `pub use pages::{...}` block:

```rust
pub use pages::{
    Page, is_page_name, member_digest, member_page, member_page_names, member_page_sizes,
    member_pages, read_edition, read_members_in_order,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-read`
Expected: PASS, all tests in the crate.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-read/src/pages.rs crates/taiyaku-read/src/lib.rs
git commit -m "Size a member's pages without reading them, and name the page rule once

The duplicate signature is a list of page sizes, so it needs what a listing
already knows: which entries are pages, and how big each one is uncompressed.
Uncompressed on both sides is what makes a directory of loose images and a cbz
of the same scan sign identically, which is the packaging the signature exists
to see through. is_image becomes public as is_page_name rather than being
copied into the CLI -- which entries are pages is normative, and a second copy
of the list is the duplication O10 was closed to remove."
```

---

### Task 2: `Signature` and the confirmation digest

**Files:**
- Create: `crates/taiyaku-cli/src/signature.rs`
- Modify: `crates/taiyaku-cli/Cargo.toml`
- Modify: `crates/taiyaku-cli/src/main.rs:18-23` (module list)

**Interfaces:**
- Consumes: `taiyaku_read::member_page_sizes`, `taiyaku_read::read_edition`,
  `taiyaku_read::natural_cmp`.
- Produces:
  - `signature::Signature { pages: u32, bytes: u64, digest: String }`, `Debug + Clone + PartialEq + Eq`
  - `Signature::of(members: &[PathBuf]) -> Result<Signature, String>`
  - `Signature::from_sizes(sizes: &[u64]) -> Signature`
  - `signature::content_digest(members: &[PathBuf]) -> Result<String, String>`

- [ ] **Step 1: Add the dependency edge**

In `crates/taiyaku-cli/Cargo.toml`, add to `[dependencies]`, keeping the list
alphabetical:

```toml
sha2 = "0.11.0"
```

It sits between `serde_json` and `taiyaku-core`.

- [ ] **Step 2: Write the failing tests**

Create `crates/taiyaku-cli/src/signature.rs` containing only the module comment
and the test module for now:

```rust
//! What a directory listing knows about two copies, before anything is
//! decoded, transferred or solved.
//!
//! This is step 1 of the duplicate protocol in `docs/status.md` §2. It settled
//! 3 of the 19 duplicated One Piece volume-editions without transferring
//! anything, which is the reason it runs first.

#[cfg(test)]
mod tests {
    use super::*;

    /// A repacker writes entries in whatever order it likes. Sorting the size
    /// list is what makes the signature independent of that, and it is the one
    /// thing about a re-packaging that always changes.
    #[test]
    fn the_order_pages_were_stored_in_does_not_change_the_signature() {
        let one = Signature::from_sizes(&[300, 100, 200]);
        let other = Signature::from_sizes(&[100, 200, 300]);

        assert_eq!(one.digest, other.digest);
        assert_eq!(one.pages, 3);
        assert_eq!(one.bytes, 600);
    }

    /// And it must still be able to tell two scans apart. One page differing by
    /// a single byte is the smallest difference there is.
    #[test]
    fn one_page_of_a_different_size_changes_the_signature() {
        let one = Signature::from_sizes(&[100, 200, 300]);
        let other = Signature::from_sizes(&[100, 200, 301]);

        assert_ne!(one.digest, other.digest);
    }

    /// The property the whole step rests on: the same scan filed two ways is
    /// one signature. Without this, repackaging defeats step 1 and every set
    /// pays for a full alignment.
    #[test]
    fn a_directory_and_an_archive_of_one_scan_sign_identically() {
        let (dir, archive) = same_pages_two_ways("sig");

        let from_dir = Signature::of(&[dir.clone()]).unwrap();
        let from_archive = Signature::of(&[archive.clone()]).unwrap();

        clean_up(&dir, &archive);
        assert_eq!(from_dir, from_archive);
    }

    /// The confirmation has to be packaging-independent for the same reason the
    /// signature is. `taiyaku_read::member_digest` is not: it hashes an
    /// archive's file bytes and a directory's page bytes, so it would report
    /// two different answers for one scan and refuse to confirm precisely the
    /// case it was called about.
    #[test]
    fn the_content_digest_sees_through_packaging_too() {
        let (dir, archive) = same_pages_two_ways("content");

        let from_dir = content_digest(&[dir.clone()]).unwrap();
        let from_archive = content_digest(&[archive.clone()]).unwrap();

        clean_up(&dir, &archive);
        assert_eq!(from_dir, from_archive);
    }

    #[test]
    fn different_page_bytes_produce_a_different_content_digest() {
        let (dir, archive) = same_pages_two_ways("differs");
        std::fs::write(dir.join("p2.jpg"), b"changed").unwrap();

        let from_dir = content_digest(&[dir.clone()]).unwrap();
        let from_archive = content_digest(&[archive.clone()]).unwrap();

        clean_up(&dir, &archive);
        assert_ne!(from_dir, from_archive);
    }

    /// One scan as a directory of loose images and as a cbz storing the same
    /// entries in a different order.
    fn same_pages_two_ways(tag: &str) -> (PathBuf, PathBuf) {
        use std::io::Write;

        let dir = std::env::temp_dir().join(format!("taiyaku-test-{tag}-dir"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
            std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
        }

        let archive = std::env::temp_dir().join(format!("taiyaku-test-{tag}.cbz"));
        let _ = std::fs::remove_file(&archive);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        for entry in ["p10.jpg", "p2.jpg", "p1.jpg"] {
            zip.start_file(entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(entry.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        (dir, archive)
    }

    fn clean_up(dir: &Path, archive: &Path) {
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_file(archive);
    }
}
```

The test module writes a cbz, so `taiyaku-cli` needs `zip` as a dev-dependency.
Add to `crates/taiyaku-cli/Cargo.toml`:

```toml
[dev-dependencies]
zip = "7.2.0"
```

Register the module in `crates/taiyaku-cli/src/main.rs`, keeping the list
alphabetical — it goes after `report`:

```rust
mod signature;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli signature::`
Expected: FAIL — `cannot find type 'Signature' in this scope`.

- [ ] **Step 4: Implement**

Insert into `crates/taiyaku-cli/src/signature.rs`, between the module comment
and `#[cfg(test)]`:

```rust
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// What a directory listing knows about one copy of a volume.
///
/// Over *pages* rather than over files, so `docs/sidecar.md`'s "How a member
/// becomes pages" decides what counts and a stray `ComicInfo.xml` or
/// `Thumbs.db` does not move the answer. Over *uncompressed* sizes, so a
/// directory of loose images and a `.cbz` of the same scan sign identically.
///
/// `pages` and `bytes` are printed because a person reads them; only `digest`
/// decides. It subsumes both, being taken over the whole sorted list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub pages: u32,
    pub bytes: u64,
    pub digest: String,
}

impl Signature {
    /// Sign one copy, which may be several members, without reading a page.
    pub fn of(members: &[PathBuf]) -> Result<Self, String> {
        let mut sizes = Vec::new();
        for member in ordered(members) {
            sizes.extend(taiyaku_read::member_page_sizes(&member)?);
        }
        Ok(Self::from_sizes(&sizes))
    }

    /// The signature of a page-size list.
    ///
    /// Sorted here rather than by the caller, because a caller that forgot
    /// would get an answer that looked right and was packaging-dependent —
    /// which is the one failure this step cannot afford, since nothing
    /// downstream re-checks it.
    pub fn from_sizes(sizes: &[u64]) -> Self {
        let mut sorted = sizes.to_vec();
        sorted.sort_unstable();
        let mut hasher = Sha256::new();
        for size in &sorted {
            hasher.update(size.to_le_bytes());
        }
        Self {
            pages: sizes.len() as u32,
            bytes: sizes.iter().sum(),
            digest: hex(hasher),
        }
    }
}

/// A hash of a copy's page bytes, in reading order: what *confirms* a signature
/// match rather than assuming it.
///
/// Not `taiyaku_read::member_digest`, which hashes an archive's file bytes and
/// a directory's page bytes. Those disagree for one scan filed two ways, which
/// is the case a signature match is most likely to be reporting, so using it
/// here would refuse to confirm exactly what it was called about. Hashing pages
/// makes the confirmation packaging-independent in the same way the signature
/// is.
///
/// One member is read and dropped at a time, so a copy costs its largest member
/// in memory rather than all of it.
pub fn content_digest(members: &[PathBuf]) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for member in ordered(members) {
        for page in taiyaku_read::read_edition(&[member])? {
            hasher.update(&page.bytes);
        }
    }
    Ok(hex(hasher))
}

/// A copy's members in the order they read, which is not the order they were
/// typed: a shell glob yields `c10.5, c10, c1, c2`. Same funnel as
/// `split_editions` in `main.rs`, and for the same reason.
fn ordered(members: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = members.to_vec();
    sorted.sort_by(|a, b| taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    sorted
}

fn hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli signature::`
Expected: PASS, 5 tests.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/Cargo.toml crates/taiyaku-cli/src/signature.rs crates/taiyaku-cli/src/main.rs
git commit -m "Sign a copy from its listing, and hash its pages to confirm

Step 1 of the duplicate protocol settled 3 of 19 One Piece sets without
transferring anything, on file count, total bytes and the sorted size list.
Signing pages rather than files means a ComicInfo.xml cannot move the answer,
and signing uncompressed sizes means repackaging cannot either.

The confirmation deliberately does not use member_digest. That hashes an
archive's file bytes and a directory's page bytes, so one scan filed two ways
hashes differently -- which is the case a signature match is most likely to be
reporting. content_digest hashes pages, so it can confirm what it was called
about. sha2 is a new edge, not a new dependency: taiyaku-read already pulls it
and status.md already names it."
```

---

### Task 3: the comparison core

**Files:**
- Create: `crates/taiyaku-cli/src/duplicates.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (module list)

**Interfaces:**
- Consumes: `taiyaku_core::{Run, Verdict, paired_slots, verdict}`.
- Produces:
  - `duplicates::Relation` — `Identity | Distinct | Refused`
  - `duplicates::Pair { a, b, a_slots, b_slots, runs, paired, verdict, relation }`
  - `Pair::judge(a: usize, b: usize, runs: &[Run], a_slots: usize, b_slots: usize) -> Pair`
  - `duplicates::cluster(copies: usize, pairs: &[Pair]) -> Vec<Vec<usize>>`
  - `duplicates::contradictions(clusters: &[Vec<usize>], pairs: &[Pair]) -> Vec<(usize, usize)>`
  - `duplicates::flagged(copies: usize, pairs: &[Pair]) -> Vec<usize>`
  - `duplicates::Against { runs: usize, paired: u32 }`
  - `duplicates::Ranked { cluster: Vec<usize>, slots: usize, against: Option<Against> }`
  - `duplicates::rank(entries: Vec<Ranked>) -> Vec<Ranked>`
  - `duplicates::decided(ranked: &[Ranked]) -> bool`

Everything in this task is pure: it takes run lists and slot counts and returns
verdicts. Nothing here opens a file, which is what lets the v077 shape and the
Chainsaw Man shape be tested without the corpus.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-cli/src/duplicates.rs` with the module comment and this
test module:

```rust
//! Resolving several copies of one volume into a decision.
//!
//! The protocol is `docs/status.md` §2, worked out by hand across 109 volumes
//! and recorded in `docs/notes.md` under "Sorting 109 volumes' worth of
//! duplicates": signature, then all pairs *within* the set, then a ranking of
//! what is genuinely distinct.
//!
//! All pairs rather than every copy against the first, because anchoring on the
//! first copy reported One Piece volume 77 as three mutually unrelated files
//! when two of them are an exact identity — and the first copy is the one most
//! likely to be the odd one out, since the sort that produced it was by page
//! count.

#[cfg(test)]
mod tests {
    use super::*;
    use taiyaku_core::PageRange;

    /// Two copies of one scan: every slot paired on both sides.
    fn whole(slots: u32) -> Vec<Run> {
        vec![Run::paired(
            PageRange::new(0, slots - 1),
            PageRange::new(0, slots - 1),
        )]
    }

    /// Identity is mutual full coverage. It is necessarily one run, because
    /// paired runs are 1:1 and an offset that never changes cannot start a
    /// second run — so the test states coverage and asserts the run count as a
    /// consequence rather than as the definition.
    #[test]
    fn full_coverage_on_both_sides_is_identity() {
        let pair = Pair::judge(0, 1, &whole(234), 234, 234);

        assert_eq!(pair.relation, Relation::Identity);
        assert_eq!(pair.runs, 1);
        assert_eq!(pair.paired, 234);
    }

    /// The distinction base selection turns on. Chainsaw Man's `v01` against
    /// `v01b` covers all 198 of A and leaves a 199th page on B that A never
    /// reaches — so they are different editions, and `v01b` goes on to win the
    /// rank on that extra page. Collapsing them would discard the difference
    /// the choice is about.
    #[test]
    fn one_slot_left_over_on_the_far_side_is_not_identity() {
        let pair = Pair::judge(0, 1, &whole(198), 198, 199);

        assert_eq!(pair.relation, Relation::Distinct);
        assert_eq!(pair.paired, 198);
    }

    /// A refusal is a claim about the volume, not about the alignment.
    #[test]
    fn nothing_paired_is_a_refusal() {
        let runs = vec![Run::gap(PageRange::new(0, 201))];

        let pair = Pair::judge(0, 1, &runs, 202, 198);

        assert_eq!(pair.relation, Relation::Refused);
        assert_eq!(pair.verdict, Verdict::Refused);
    }

    /// One Piece volume 77, which is the case that argues for all-pairs: c1 and
    /// c2 are the same edition at different paginations and c0 is a different
    /// crop. Anchoring on c0 reports three unrelated files.
    #[test]
    fn identity_pairs_collapse_and_the_outlier_stays_alone() {
        let pairs = vec![
            Pair::judge(0, 1, &fragmented(), 236, 234),
            Pair::judge(0, 2, &fragmented(), 236, 234),
            Pair::judge(1, 2, &whole(234), 234, 234),
        ];

        let clusters = cluster(3, &pairs);

        assert_eq!(clusters, vec![vec![0], vec![1, 2]]);
        assert!(contradictions(&clusters, &pairs).is_empty());
    }

    /// Identity is transitive or one of the three solves is wrong. Every pair
    /// is solved anyway, so the contradiction is already in hand: report it and
    /// keep the merge, which is still the best available answer.
    #[test]
    fn identity_that_is_not_transitive_is_reported_and_still_merged() {
        let pairs = vec![
            Pair::judge(0, 1, &whole(200), 200, 200),
            Pair::judge(1, 2, &whole(200), 200, 200),
            Pair::judge(0, 2, &fragmented(), 200, 200),
        ];

        let clusters = cluster(3, &pairs);

        assert_eq!(clusters, vec![vec![0, 1, 2]]);
        assert_eq!(contradictions(&clusters, &pairs), vec![(0, 2)]);
    }

    /// The Bocchi Gaiden file and the One Piece Starter Book: named for a
    /// volume, refused by every copy of it.
    #[test]
    fn a_copy_that_refuses_against_everything_is_flagged() {
        let pairs = vec![
            Pair::judge(0, 1, &whole(124), 124, 124),
            Pair::judge(0, 2, &refusal(124), 124, 185),
            Pair::judge(1, 2, &refusal(124), 124, 185),
        ];

        assert_eq!(flagged(3, &pairs), vec![2]);
    }

    /// With two copies and a refusal between them, nothing in the set says
    /// which one is the impostor, so both are flagged and nothing is ranked.
    /// `--against` is what resolves it.
    #[test]
    fn two_copies_that_refuse_each_other_flag_both() {
        let pairs = vec![Pair::judge(0, 1, &refusal(202), 202, 198)];

        assert_eq!(flagged(2, &pairs), vec![0, 1]);
    }

    /// Content first. `notes.md`: "a missing page cannot be recovered from a
    /// cleaner alignment of the pages that remain".
    #[test]
    fn ranking_puts_the_copy_with_more_slots_first() {
        let ranked = rank(vec![
            Ranked { cluster: vec![1], slots: 234, against: None },
            Ranked { cluster: vec![0], slots: 236, against: None },
        ]);

        assert_eq!(ranked[0].cluster, vec![0]);
        assert!(decided(&ranked));
    }

    /// Without a reference there is no honest tiebreak, so a tie is reported as
    /// a tie rather than broken by a statistic that decides nothing.
    #[test]
    fn a_tie_on_slots_with_no_reference_is_undecided() {
        let ranked = rank(vec![
            Ranked { cluster: vec![0], slots: 198, against: None },
            Ranked { cluster: vec![1], slots: 198, against: None },
        ]);

        assert!(!decided(&ranked));
    }

    /// The Chainsaw Man table: four scans against one English release, ranked
    /// on slots then pairings then runs. `v01b` wins on every axis at once.
    #[test]
    fn a_reference_edition_breaks_a_tie_on_pairings_then_runs() {
        let ranked = rank(vec![
            Ranked {
                cluster: vec![0],
                slots: 198,
                against: Some(Against { runs: 5, paired: 182 }),
            },
            Ranked {
                cluster: vec![1],
                slots: 199,
                against: Some(Against { runs: 3, paired: 183 }),
            },
            Ranked {
                cluster: vec![2],
                slots: 198,
                against: Some(Against { runs: 7, paired: 182 }),
            },
        ]);

        let order: Vec<usize> = ranked.iter().map(|r| r.cluster[0]).collect();
        assert_eq!(order, vec![1, 0, 2], "199 slots first, then 5 runs before 7");
        assert!(decided(&ranked));
    }

    /// A hundred short runs over most of the pages: the shape a needle and a
    /// differently-cropped scan both produce.
    fn fragmented() -> Vec<Run> {
        (0..100)
            .map(|i| Run::paired(PageRange::new(i * 2, i * 2), PageRange::new(i * 2, i * 2)))
            .collect()
    }

    fn refusal(a_slots: u32) -> Vec<Run> {
        vec![Run::gap(PageRange::new(0, a_slots - 1))]
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli duplicates::`
Expected: FAIL — `cannot find type 'Pair' in this scope`.

- [ ] **Step 3: Implement**

Insert into `crates/taiyaku-cli/src/duplicates.rs` between the module comment
and `#[cfg(test)]`:

```rust
use taiyaku_core::{Run, Verdict, paired_slots, verdict};

/// How two copies stand to one another, decided from one solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Every slot paired on both sides: the same edition, however it is packed.
    Identity,
    /// Paired, but not wholly. Genuinely different editions of one volume.
    Distinct,
    /// Nothing paired. Not this volume at all.
    Refused,
}

/// One solved pair, kept for the table and for the clustering.
#[derive(Debug, Clone)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub a_slots: usize,
    pub b_slots: usize,
    pub runs: usize,
    pub paired: u32,
    pub verdict: Verdict,
    pub relation: Relation,
}

impl Pair {
    /// Judge one solve.
    ///
    /// Identity is stated as coverage rather than as `runs == 1` because
    /// coverage is the property that matters and the single run is a
    /// consequence: paired runs are 1:1, so full coverage on both sides forces
    /// a constant offset and an offset that never changes is one run. If a
    /// solve ever reported full mutual coverage across several runs, that would
    /// mean run compression is broken — worth finding out rather than papering
    /// over, which is why the corpus check asserts the run count separately.
    pub fn judge(a: usize, b: usize, runs: &[Run], a_slots: usize, b_slots: usize) -> Self {
        let paired = paired_slots(runs);
        let call = verdict(runs, a_slots);
        let relation = match call {
            Verdict::Refused => Relation::Refused,
            _ if paired as usize == a_slots && paired as usize == b_slots => Relation::Identity,
            _ => Relation::Distinct,
        };
        Self {
            a,
            b,
            a_slots,
            b_slots,
            runs: runs.len(),
            paired,
            verdict: call,
            relation,
        }
    }

    fn touches(&self, copy: usize) -> bool {
        self.a == copy || self.b == copy
    }
}

/// Collapse copies that are the same edition, by union-find over the identity
/// pairs.
///
/// Clusters come back with their members ascending, ordered by their lowest
/// member, so the output is stable whatever order the pairs arrived in.
pub fn cluster(copies: usize, pairs: &[Pair]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..copies).collect();
    fn root(parent: &mut Vec<usize>, mut of: usize) -> usize {
        while parent[of] != of {
            parent[of] = parent[parent[of]];
            of = parent[of];
        }
        of
    }
    for pair in pairs.iter().filter(|p| p.relation == Relation::Identity) {
        let (a, b) = (root(&mut parent, pair.a), root(&mut parent, pair.b));
        if a != b {
            parent[a.max(b)] = a.min(b);
        }
    }

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for copy in 0..copies {
        let of = root(&mut parent, copy);
        match groups.iter_mut().find(|g| root(&mut parent, g[0]) == of) {
            Some(group) => group.push(copy),
            None => groups.push(vec![copy]),
        }
    }
    groups
}

/// Pairs the clustering merged that the solve did not call identity.
///
/// `A ≡ B` and `B ≡ C` with `A ≢ C` is a contradiction, and every pair is
/// solved anyway so it is already in hand. Reported rather than resolved: the
/// merge is still the best available answer, and the contradiction means one of
/// the three alignments is wrong — a thing to go and look at, not a reason to
/// refuse to report.
pub fn contradictions(clusters: &[Vec<usize>], pairs: &[Pair]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for group in clusters.iter().filter(|g| g.len() > 1) {
        for (at, &a) in group.iter().enumerate() {
            for &b in &group[at + 1..] {
                let judged = pairs
                    .iter()
                    .find(|p| (p.a == a && p.b == b) || (p.a == b && p.b == a));
                if judged.is_some_and(|p| p.relation != Relation::Identity) {
                    found.push((a, b));
                }
            }
        }
    }
    found
}

/// Copies that refused against every other copy: not this volume.
///
/// With two copies and a refusal between them both are flagged, because nothing
/// in the set says which one is the impostor. That is the honest answer, and
/// `--against` is what resolves it.
pub fn flagged(copies: usize, pairs: &[Pair]) -> Vec<usize> {
    (0..copies)
        .filter(|&copy| {
            let mut own = pairs.iter().filter(|p| p.touches(copy)).peekable();
            own.peek().is_some() && own.all(|p| p.relation == Relation::Refused)
        })
        .collect()
}

/// How a cluster's representative aligned against the reference edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Against {
    pub runs: usize,
    pub paired: u32,
}

/// One row of the ranking: a cluster, its content, and how cleanly it aligns.
#[derive(Debug, Clone)]
pub struct Ranked {
    pub cluster: Vec<usize>,
    pub slots: usize,
    pub against: Option<Against>,
}

/// Rank clusters: content first, cleanliness only as a tiebreak.
///
/// `notes.md`, on the Chainsaw Man result: "Pages and pairings measure content;
/// runs measure how cleanly the correspondence holds. Content should win,
/// because a missing page cannot be recovered from a cleaner alignment of the
/// pages that remain."
pub fn rank(mut entries: Vec<Ranked>) -> Vec<Ranked> {
    entries.sort_by(|x, y| {
        y.slots.cmp(&x.slots).then_with(|| match (x.against, y.against) {
            (Some(a), Some(b)) => b.paired.cmp(&a.paired).then(a.runs.cmp(&b.runs)),
            _ => std::cmp::Ordering::Equal,
        })
    });
    entries
}

/// Whether the ranking has a unique winner.
///
/// A tie at the top is reported as a tie rather than broken by a statistic that
/// decides nothing — within-set cleanliness is symmetric and useless with two
/// clusters, and dressing it up as a decision would be worse than saying so.
pub fn decided(ranked: &[Ranked]) -> bool {
    match ranked {
        [] => false,
        [_] => true,
        [first, second, ..] => {
            first.slots != second.slots
                || match (first.against, second.against) {
                    (Some(a), Some(b)) => (a.paired, b.runs) != (b.paired, a.runs),
                    _ => false,
                }
        }
    }
}
```

Register the module in `crates/taiyaku-cli/src/main.rs`, alphabetically after
`mod corpus;`:

```rust
mod duplicates;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli duplicates::`
Expected: PASS, 10 tests.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/duplicates.rs crates/taiyaku-cli/src/main.rs
git commit -m "Judge, cluster, flag and rank several copies of one volume

Identity is every slot paired on both sides, which is necessarily one run and
is stated as coverage so that a solve reporting otherwise shows up as the run
compression bug it would be. That definition is what makes base selection come
out right: Chainsaw Man's v01 covers all 198 of its own slots against v01b and
leaves a 199th on the far side, so the two stay distinct and v01b wins the rank
on the extra page.

Clustering is union-find over the identity pairs. Non-transitive identity is
reported and still merged -- the merge is the best answer available and the
contradiction means one of three alignments is wrong. A copy that refuses
against every other copy is flagged rather than ranked, which is the Bocchi
Gaiden file and the One Piece Starter Book; with two copies and a refusal
between them both are flagged, because nothing in the set says which is which."
```

---

### Task 4: the two output forms

**Files:**
- Modify: `crates/taiyaku-cli/src/duplicates.rs`

**Interfaces:**
- Consumes: everything from Task 3, plus `crate::stats::verdict_word`.
- Produces:
  - `duplicates::Resolution { copies, signature_settled, pairs, clusters, contradictions, flagged, ranked }`
  - `duplicates::Copy { label: String, slots: usize }`
  - `duplicates::tables(r: &Resolution) -> String`
  - `duplicates::set_line(r: &Resolution) -> String`

**Why the field order.** `best` is **last** so a path containing a space cannot
break field splitting for everything after it. Every other field is a number or
a word.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-cli/src/duplicates.rs`:

```rust
    /// One Piece volume 77 as actually measured: c0 is a different crop at 236
    /// slots, c1 and c2 are one edition at two paginations. The line a survey
    /// script collects, asserted verbatim — `scripts/margin-survey.sh` reads
    /// `stats:` lines exactly this way, and a format nobody can test drifts.
    #[test]
    fn the_set_line_reports_every_figure_that_decided_the_answer() {
        let line = set_line(&v077());

        assert_eq!(
            line,
            "set: copies=3 signature=0 clusters=2 flagged=0 refused=0 fragmented=2 \
             slots=236 decided=yes best=op/v077-c0"
        );
    }

    /// Nothing ranked still produces a line a survey loop can parse, rather
    /// than vanishing.
    #[test]
    fn a_set_with_nothing_ranked_still_produces_a_line() {
        let resolution = Resolution {
            copies: vec![
                Copy { label: "op/v002-c0".into(), slots: 202 },
                Copy { label: "op/v002-c1".into(), slots: 129 },
            ],
            signature_settled: 0,
            pairs: vec![Pair::judge(0, 1, &refusal(202), 202, 129)],
            clusters: vec![vec![0], vec![1]],
            contradictions: Vec::new(),
            flagged: vec![0, 1],
            ranked: Vec::new(),
        };

        assert_eq!(
            set_line(&resolution),
            "set: copies=2 signature=0 clusters=2 flagged=2 refused=1 fragmented=0 \
             slots=- decided=no best=-"
        );
    }

    #[test]
    fn the_tables_show_the_pairs_the_clusters_and_the_rank() {
        let printed = tables(&v077());

        assert_eq!(
            printed,
            "\
copies
  0  236 slots  op/v077-c0
  1  234 slots  op/v077-c1
  2  234 slots  op/v077-c2

pairs
  0-1  100 runs  184 of 236  FRAGMENTED
  0-2  100 runs  186 of 236  FRAGMENTED
  1-2  1 run     234 of 234  sound  identity

clusters
  {0}
  {1, 2}  identity

rank
  1.  236 slots  copy 0
  2.  234 slots  copy 1
"
        );
    }

    /// The v077 resolution, built from the shapes rather than from files.
    fn v077() -> Resolution {
        let pairs = vec![
            Pair::judge(0, 1, &spanning(100, 184), 236, 234),
            Pair::judge(0, 2, &spanning(100, 186), 236, 234),
            Pair::judge(1, 2, &whole(234), 234, 234),
        ];
        let clusters = cluster(3, &pairs);
        Resolution {
            copies: vec![
                Copy { label: "op/v077-c0".into(), slots: 236 },
                Copy { label: "op/v077-c1".into(), slots: 234 },
                Copy { label: "op/v077-c2".into(), slots: 234 },
            ],
            signature_settled: 0,
            contradictions: contradictions(&clusters, &pairs),
            flagged: flagged(3, &pairs),
            ranked: rank(vec![
                Ranked { cluster: vec![0], slots: 236, against: None },
                Ranked { cluster: vec![1, 2], slots: 234, against: None },
            ]),
            clusters,
            pairs,
        }
    }

    /// Exactly `runs` paired runs covering exactly `paired` slots between them,
    /// each separated by a slot of gap so they stay separate runs.
    ///
    /// The remainder is spread over the leading runs rather than dropped: a
    /// helper that quietly covered 100 slots when the test said 184 would make
    /// the assertion pass for the wrong reason. Assumes `paired >= runs`, which
    /// every caller satisfies.
    fn spanning(runs: u32, paired: u32) -> Vec<Run> {
        let (each, longer) = (paired / runs, paired % runs);
        let mut out = Vec::new();
        let mut at = 0;
        for run in 0..runs {
            let length = each + u32::from(run < longer);
            out.push(Run::paired(
                PageRange::new(at, at + length - 1),
                PageRange::new(at, at + length - 1),
            ));
            at += length + 1;
        }
        out
    }
```

Keep the `fragmented()` helper — `identity_pairs_collapse_and_the_outlier_stays_alone`
and `identity_that_is_not_transitive_is_reported_and_still_merged` both use it.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli duplicates::`
Expected: FAIL — `cannot find type 'Resolution' in this scope`.

- [ ] **Step 3: Implement**

Add these two lines to the import block at the top of
`crates/taiyaku-cli/src/duplicates.rs`, beside the `taiyaku_core` import:

```rust
use std::fmt::Write as _;

use crate::stats::verdict_word;
```

Then add after `decided`:

```rust
/// One copy, as far as the output is concerned.
#[derive(Debug, Clone)]
pub struct Copy {
    /// The path as the caller gave it, for a person to act on.
    pub label: String,
    pub slots: usize,
}

/// Everything one set resolved to.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub copies: Vec<Copy>,
    /// Copies removed by the signature step, before any alignment.
    pub signature_settled: usize,
    pub pairs: Vec<Pair>,
    pub clusters: Vec<Vec<usize>>,
    pub contradictions: Vec<(usize, usize)>,
    pub flagged: Vec<usize>,
    pub ranked: Vec<Ranked>,
}

/// The one line a survey script collects.
///
/// Fixed field order and no wrapping, the discipline `stats::calibration_line`
/// already establishes and `scripts/margin-survey.sh` already depends on.
/// `best` is **last** so that a path containing a space cannot break field
/// splitting for anything after it; every other field is a number or a word.
pub fn set_line(resolution: &Resolution) -> String {
    let count = |wanted: Verdict| {
        resolution
            .pairs
            .iter()
            .filter(|p| p.verdict == wanted)
            .count()
    };
    let (slots, best) = match resolution.ranked.first() {
        Some(first) => (
            first.slots.to_string(),
            resolution.copies[first.cluster[0]].label.clone(),
        ),
        None => ("-".to_string(), "-".to_string()),
    };
    format!(
        "set: copies={} signature={} clusters={} flagged={} refused={} fragmented={} \
         slots={slots} decided={} best={best}",
        resolution.copies.len(),
        resolution.signature_settled,
        resolution.clusters.len(),
        resolution.flagged.len(),
        count(Verdict::Refused),
        count(Verdict::Fragmented),
        match decided(&resolution.ranked) {
            true => "yes",
            false => "no",
        }
    )
}

/// The readable form: what was compared, what collapsed, and what is left.
pub fn tables(resolution: &Resolution) -> String {
    let mut out = String::from("copies\n");
    for (index, copy) in resolution.copies.iter().enumerate() {
        let _ = writeln!(out, "  {index}  {} slots  {}", copy.slots, copy.label);
    }

    if !resolution.pairs.is_empty() {
        out.push_str("\npairs\n");
        for pair in &resolution.pairs {
            let _ = writeln!(
                out,
                "  {}-{}  {:<9} {} of {}  {}{}",
                pair.a,
                pair.b,
                plural(pair.runs),
                pair.paired,
                pair.a_slots,
                verdict_word(pair.verdict),
                match pair.relation {
                    Relation::Identity => "  identity",
                    _ => "",
                }
            );
        }
    }

    out.push_str("\nclusters\n");
    for group in &resolution.clusters {
        let members: Vec<String> = group.iter().map(usize::to_string).collect();
        let _ = writeln!(
            out,
            "  {{{}}}{}",
            members.join(", "),
            match group.len() > 1 {
                true => "  identity",
                false => "",
            }
        );
    }

    for (a, b) in &resolution.contradictions {
        let _ = writeln!(
            out,
            "\ncopies {a} and {b} were merged but did not align as identity; \
             one of the three solves in that cluster is wrong"
        );
    }

    if !resolution.flagged.is_empty() {
        out.push_str("\nnot this volume\n");
        for &copy in &resolution.flagged {
            let _ = writeln!(
                out,
                "  {copy}  refused against every other copy  {}",
                resolution.copies[copy].label
            );
        }
        if resolution.flagged.len() == resolution.copies.len() {
            out.push_str(
                "  nothing in this set says which of these is the impostor; \
                 pass --against a reference edition\n",
            );
        }
    }

    if !resolution.ranked.is_empty() {
        out.push_str("\nrank\n");
        for (place, entry) in resolution.ranked.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {}.  {} slots  copy {}{}",
                place + 1,
                entry.slots,
                entry.cluster[0],
                match entry.against {
                    Some(a) => format!("  {} paired  {}", a.paired, plural(a.runs)),
                    None => String::new(),
                }
            );
        }
    }

    out
}

fn plural(runs: usize) -> String {
    match runs {
        1 => "1 run".to_string(),
        n => format!("{n} runs"),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli duplicates::`
Expected: PASS, 13 tests.

Column widths are a free choice; the assertion exists to pin whichever one is
chosen. If the implementation's widths differ from the expected string above,
**decide** which widths the table should have, then make the code and the
assertion agree on that decision. Do not repeatedly re-paste actual output into
the assertion until it passes — that is a test that can no longer fail.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/duplicates.rs
git commit -m "Print a set for a person, and one fixed-field line for a survey

The tables go to a reader and the set: line to a script, the same split align
--stats already makes and scripts/margin-survey.sh already depends on. best is
the last field deliberately: a corpus path can contain a space, and putting it
anywhere else would break field splitting for everything after it.

A set with nothing ranked still emits a line, because a survey loop that sees
nineteen sets and eighteen lines cannot tell which one it lost."
```

---

### Task 5: the `duplicates` command

**Files:**
- Modify: `crates/taiyaku-cli/src/duplicates.rs`
- Modify: `crates/taiyaku-cli/src/main.rs:40-70` (dispatch and usage)

**Interfaces:**
- Consumes: `crate::cached_edition`, `crate::signature::{Signature, content_digest}`,
  `taiyaku_core::{Direction, solve}`, everything from Tasks 3 and 4.
- Produces:
  - `duplicates::command(args: &[String]) -> Result<(), String>`
  - `duplicates::resolve(copies: &[Vec<PathBuf>], against: Option<&[PathBuf]>, direction: Direction, cache: &Path) -> Result<Resolution, String>`
  - `duplicates::groups(args: &[String]) -> Result<Vec<Vec<PathBuf>>, String>`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-cli/src/duplicates.rs`:

```rust
    /// Copies are `--`-separated as in `topology`, but unlike `topology` this
    /// command has flags — and `--against` itself begins with `--`. Group
    /// collection therefore stops at the first argument that starts with `--`
    /// and is not exactly `--`.
    #[test]
    fn groups_end_at_the_first_flag() {
        let args: Vec<String> = ["a", "--", "b1", "b2", "--against", "ref", "--ltr"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let parsed = groups(&args).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].len(), 2, "both b members, and not `ref`");
    }

    #[test]
    fn one_copy_is_not_a_duplicate_set() {
        let args: Vec<String> = ["only"].iter().map(|s| s.to_string()).collect();

        let error = groups(&args).unwrap_err();

        assert!(error.contains("two copies"), "unhelpful: {error}");
    }

    /// The saving the signature step exists for. Three copies of one scan
    /// fingerprint once, not three times — skipping a solve saves microseconds
    /// and skipping a decode of every page saves seconds.
    #[test]
    fn copies_confirmed_identical_all_stand_for_the_first() {
        let same = vec![signed("aaa"), signed("aaa"), signed("aaa")];
        let mut confirmed = 0;

        let stands_for = representatives(&same, |_, _| {
            confirmed += 1;
            Ok(true)
        })
        .unwrap();

        assert_eq!(stands_for, vec![0, 0, 0]);
        assert_eq!(confirmed, 2, "each later copy confirmed against the first only");
    }

    /// The signature is a claim about sizes, not about bytes. When the content
    /// hash refutes it the copies stay separate and get solved like any other
    /// pair — the confirmation is what makes the short-circuit sound rather
    /// than merely fast.
    #[test]
    fn a_signature_match_the_content_hash_refutes_does_not_collapse() {
        let same = vec![signed("aaa"), signed("aaa")];

        let stands_for = representatives(&same, |_, _| Ok(false)).unwrap();

        assert_eq!(stands_for, vec![0, 1]);
    }

    /// And the expensive confirmation is never reached where a listing already
    /// settled it.
    #[test]
    fn copies_with_different_signatures_are_never_hashed() {
        let different = vec![signed("aaa"), signed("bbb")];

        let stands_for = representatives(&different, |_, _| {
            panic!("a differing signature must not cost a content hash")
        })
        .unwrap();

        assert_eq!(stands_for, vec![0, 1]);
    }

    fn signed(digest: &str) -> Signature {
        Signature {
            pages: 3,
            bytes: 600,
            digest: digest.to_string(),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli duplicates::`
Expected: FAIL — `cannot find function 'groups' in this scope` and
`cannot find function 'representatives' in this scope`.

- [ ] **Step 3: Implement the parsing and the wiring**

Add to `crates/taiyaku-cli/src/duplicates.rs`:

```rust
use std::path::{Path, PathBuf};

use taiyaku_core::{Direction, PageHash, PageRange, solve};

use crate::signature::{Signature, content_digest};

/// The `--`-separated copies, stopping at the first flag.
///
/// `topology` splits on `--` throughout because it has no flags. This command
/// does, and `--against` begins with `--`, so the groups have to end somewhere:
/// at the first argument that starts with `--` and is not exactly `--`.
///
/// Members within a copy come back in natural order rather than as typed, for
/// the reason `split_editions` gives — a shell glob yields `c10.5, c10, c1, c2`
/// and a copy is a page sequence.
pub fn groups(args: &[String]) -> Result<Vec<Vec<PathBuf>>, String> {
    let end = args
        .iter()
        .position(|a| a.starts_with("--") && a != "--")
        .unwrap_or(args.len());
    let mut copies: Vec<Vec<PathBuf>> = args[..end]
        .split(|a| a == "--")
        .filter(|g| !g.is_empty())
        .map(|g| g.iter().map(PathBuf::from).collect())
        .collect();
    if copies.len() < 2 {
        return Err(
            "needs at least two copies, separated by `--`; one copy is not a duplicate set"
                .to_string(),
        );
    }
    for copy in &mut copies {
        copy.sort_by(|a, b| taiyaku_read::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    }
    Ok(copies)
}

/// The members following `--against`, up to the next flag.
fn against_members(args: &[String]) -> Option<Vec<PathBuf>> {
    let at = args.iter().position(|a| a == "--against")?;
    let members: Vec<PathBuf> = args[at + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    (!members.is_empty()).then_some(members)
}

/// Which copy each copy stands for: itself, or the first earlier copy it was
/// confirmed to hold the same bytes as.
///
/// The signature is checked first and the confirmation only where it matched,
/// so a set of genuinely different copies pays for a listing and nothing more.
/// `confirm` is a parameter rather than a direct call to `content_digest` so
/// that this grouping — the thing that decides how much decoding happens — can
/// be tested without fixtures.
///
/// A copy only ever stands for a copy that stands for itself, so the result is
/// one level deep and needs no chasing.
fn representatives(
    signatures: &[Signature],
    mut confirm: impl FnMut(usize, usize) -> Result<bool, String>,
) -> Result<Vec<usize>, String> {
    let mut stands_for: Vec<usize> = (0..signatures.len()).collect();
    for copy in 0..signatures.len() {
        for earlier in 0..copy {
            if stands_for[earlier] == earlier
                && signatures[earlier].digest == signatures[copy].digest
                && confirm(earlier, copy)?
            {
                stands_for[copy] = earlier;
                break;
            }
        }
    }
    Ok(stands_for)
}

pub fn command(args: &[String]) -> Result<(), String> {
    let copies = groups(args)?;
    let direction = match args.iter().any(|a| a == "--ltr") {
        true => Direction::LeftToRight,
        false => Direction::RightToLeft,
    };
    let cache = PathBuf::from(
        crate::option_value(args, "--cache").unwrap_or(".taiyaku-cache"),
    );
    let against = against_members(args);

    let resolution = resolve(&copies, against.as_deref(), direction, &cache)?;

    eprint!("{}", tables(&resolution));
    println!("{}", set_line(&resolution));
    Ok(())
}

/// Run the protocol over one set.
///
/// Spreads always split. One Piece v092 is 200 images in one copy and 101 in
/// the other and comes out identity — but only through slot expansion, and
/// `notes.md` draws the conclusion directly: page counts are a property of
/// packaging, slot counts a property of the work. A duplicate command that
/// compared pages would get its own headline case wrong, so there is no flag
/// for this.
pub fn resolve(
    copies: &[Vec<PathBuf>],
    against: Option<&[PathBuf]>,
    direction: Direction,
    cache: &Path,
) -> Result<Resolution, String> {
    // Step 1. Cheap, and it decides what the rest has to do.
    let signatures: Vec<Signature> = copies
        .iter()
        .map(|copy| Signature::of(copy))
        .collect::<Result<_, _>>()?;

    // Which copy each copy stands for. Resolved before anything is
    // fingerprinted, because that is where the saving is: skipping a solve
    // saves microseconds and skipping a decode of every page saves seconds.
    let stands_for = representatives(&signatures, |a, b| {
        Ok(content_digest(&copies[a])? == content_digest(&copies[b])?)
    })?;
    let settled = (0..copies.len()).filter(|&c| stands_for[c] != c).count();

    // Fingerprint each representative once. A copy standing for another holds
    // the same page bytes, so it has the same dimensions, the same spreads and
    // the same slot count — there is nothing left for a decode to find.
    let mut hashes: Vec<Vec<PageHash>> = Vec::new();
    let mut described = Vec::new();
    for (copy, members) in copies.iter().enumerate() {
        let slot_hashes = match stands_for[copy] == copy {
            true => crate::cached_edition(cache, members, direction)?.0,
            false => hashes[stands_for[copy]].clone(),
        };
        described.push(Copy {
            label: members
                .first()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            slots: slot_hashes.len(),
        });
        hashes.push(slot_hashes);
    }

    // Step 2. Every unordered pair, one solve each — leaning on the symmetry
    // `docs/status.md` records as measured across five pairs in two series.
    // Two copies confirmed byte-identical are not solved: the solver would be
    // comparing a page sequence with itself, and the run describing that is the
    // whole of it.
    let mut pairs = Vec::new();
    for a in 0..copies.len() {
        for b in a + 1..copies.len() {
            let slots = described[a].slots;
            let runs = match stands_for[a] == stands_for[b] && slots > 0 {
                true => vec![Run::paired(
                    PageRange::new(0, slots as u32 - 1),
                    PageRange::new(0, slots as u32 - 1),
                )],
                false => solve(&hashes[a], &hashes[b]),
            };
            pairs.push(Pair::judge(
                a,
                b,
                &runs,
                described[a].slots,
                described[b].slots,
            ));
        }
    }

    let clusters = cluster(copies.len(), &pairs);
    let contradicting = contradictions(&clusters, &pairs);
    let out = flagged(copies.len(), &pairs);

    // Step 3. One alignment per representative, not per copy: an identity
    // cluster's members are the same content by measurement, so aligning both
    // against the reference would spend a solve to get the same answer twice.
    // The representative is whichever member came first in the argument list —
    // members of one cluster have equal slot counts by the definition of
    // identity, so the choice only has to be stable and explainable.
    let reference = match against {
        Some(members) => Some(crate::cached_edition(cache, members, direction)?.0),
        None => None,
    };
    let mut ranked = Vec::new();
    for group in &clusters {
        let first = group[0];
        if out.contains(&first) {
            continue;
        }
        let scored = reference.as_ref().map(|reference| {
            let runs = solve(&hashes[first], reference);
            Against {
                runs: runs.len(),
                paired: paired_slots(&runs),
            }
        });
        ranked.push(Ranked {
            cluster: group.clone(),
            slots: described[first].slots,
            against: scored,
        });
    }

    Ok(Resolution {
        copies: described,
        signature_settled: settled,
        pairs,
        clusters,
        contradictions: contradicting,
        flagged: out,
        ranked: rank(ranked),
    })
}
```

`option_value` in `main.rs` is currently private to that module. Make it
`pub(crate)` — change `fn option_value` at `crates/taiyaku-cli/src/main.rs:102`
to `pub(crate) fn option_value`. Do the same for `cached_edition` at line 727 if
it is not already reachable; `corpus.rs` already does `use crate::cached_edition`,
so it is at least crate-visible.

- [ ] **Step 4: Wire the command into `main.rs`**

In `run`, add the arm after `Some("serve")`:

```rust
        Some("duplicates") => duplicates::command(&args[1..]),
```

And in the usage block, after the `serve` line:

```rust
            println!(
                "       taiyaku duplicates <copy>... -- <copy>... [--against <member>...] [--ltr]"
            );
```

- [ ] **Step 5: Run everything**

Run: `cargo test -p taiyaku-cli`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no output, exit 0.

Run it for real against two copies of one volume:

```bash
cargo run -p taiyaku-cli -- duplicates \
  corpus/incoming/op-duplicates/v092-mono-c0 -- corpus/incoming/op-duplicates/v092-mono-c1
```
Expected: the tables on stderr showing one identity cluster, and a `set:` line
on stdout ending `decided=yes`.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/duplicates.rs crates/taiyaku-cli/src/main.rs
git commit -m "Run the duplicate protocol over one set as a command

Signature first, then every unordered pair -- one solve each, on the symmetry
status.md records as measured -- then a ranking of what is genuinely distinct.
A pair whose signatures match is confirmed with a content hash rather than
assumed, which is far cheaper than the solve it replaces.

Spreads always split and there is no flag for it. v092 is 200 images against
101 and comes out identity only through slot expansion, and notes.md draws the
conclusion: page counts are a property of packaging, slot counts a property of
the work. A duplicate command comparing pages would get its own headline case
wrong."
```

---

### Task 6: the `inventory` command

**Files:**
- Create: `crates/taiyaku-cli/src/inventory.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (module list, dispatch, usage)

**Interfaces:**
- Consumes: `taiyaku_read::is_page_name`, `crate::signature::Signature`.
- Produces:
  - `inventory::volume_number(name: &str) -> Option<u32>`
  - `inventory::Pattern`, `Pattern::parse(spec: &str) -> Result<Pattern, String>`, `Pattern::find(&self, name: &str) -> Option<u32>`
  - `inventory::Candidate { path: PathBuf, pages: u32 }`
  - `inventory::walk(root: &Path, min_pages: u32) -> Result<Walked, String>`
  - `inventory::Walked { dirs: usize, candidates: Vec<Candidate>, excluded: Vec<u32> }`
  - `inventory::command(args: &[String]) -> Result<(), String>`

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-cli/src/inventory.rs` with the module comment and tests:

```rust
//! The step `docs/status.md` §2 leaves out: getting from a release directory to
//! the sets worth comparing.
//!
//! On the One Piece collection that was 579 directories down to 141 volume
//! directories, forming 120 volume-editions of which 19 had more than one copy.
//! Both of those decisions are heuristics — a page-count threshold and a volume
//! number parsed from a name — which is exactly why they live here, in their own
//! command, printing what they decided, rather than inside `duplicates` where a
//! wrong guess would be invisible.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_number_is_taken_from_the_marker_that_precedes_it() {
        assert_eq!(volume_number("v069"), Some(69));
        assert_eq!(volume_number("v095-colour-c0"), Some(95));
        assert_eq!(volume_number("第69巻"), Some(69));
        assert_eq!(volume_number("vol. 7"), Some(7));
        assert_eq!(volume_number("raw-v01-src-a"), Some(1));
    }

    /// With no marker, the first digit run is the best available guess.
    #[test]
    fn a_bare_number_still_groups() {
        assert_eq!(volume_number("069"), Some(69));
    }

    /// The case a naive "last digit run" rule gets wrong, and the reason the
    /// marker is searched for before falling back.
    #[test]
    fn a_year_in_the_name_does_not_win_over_the_volume() {
        assert_eq!(volume_number("ONE PIECE v077 (2015)"), Some(77));
    }

    /// A candidate that reached the page threshold and then vanished from the
    /// output is the silence this repository exists to avoid.
    #[test]
    fn a_name_with_no_digits_has_no_number_rather_than_a_wrong_one() {
        assert_eq!(volume_number("extras"), None);
    }

    #[test]
    fn a_pattern_overrides_the_default_parse() {
        let pattern = Pattern::parse("第{}巻").unwrap();

        assert_eq!(pattern.find("第69巻"), Some(69));
        assert_eq!(pattern.find("v69"), None, "the literal must match too");
    }

    #[test]
    fn a_pattern_without_a_placeholder_is_refused() {
        let error = Pattern::parse("v").unwrap_err();

        assert!(error.contains("{}"), "unhelpful: {error}");
    }

    /// The candidate rule: the deepest directory that still holds a whole
    /// edition. Stated that way rather than as "a directory that directly holds
    /// images" because `docs/sidecar.md` records that some editions nest — a
    /// volume split into chapter directories holds no images of its own, and
    /// the chapter directories are not what a person means by a copy.
    #[test]
    fn a_nested_volume_is_one_candidate_not_seven_and_not_zero() {
        let root = std::env::temp_dir().join("taiyaku-test-inventory-nested");
        let _ = std::fs::remove_dir_all(&root);
        let volume = root.join("v01");
        for chapter in ["c001", "c002", "c003"] {
            let dir = volume.join(chapter);
            std::fs::create_dir_all(&dir).unwrap();
            for page in 0..4 {
                std::fs::write(dir.join(format!("{page}.jpg")), b"x").unwrap();
            }
        }

        let walked = walk(&root, 10).unwrap();

        let _ = std::fs::remove_dir_all(&root);
        let paths: Vec<&Path> = walked.candidates.iter().map(|c| c.path.as_path()).collect();
        assert_eq!(paths, vec![volume.as_path()]);
        assert_eq!(walked.candidates[0].pages, 12);
        assert!(walked.excluded.is_empty(), "chapters are not excluded copies");
    }

    /// What falls below the threshold is counted and its page range reported,
    /// so a threshold that nearly cut something real is visible.
    #[test]
    fn material_below_the_threshold_is_excluded_and_its_range_recorded() {
        let root = std::env::temp_dir().join("taiyaku-test-inventory-short");
        let _ = std::fs::remove_dir_all(&root);
        for (name, pages) in [("v01", 12u32), ("jump-24", 3), ("jump-31", 5)] {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            for page in 0..pages {
                std::fs::write(dir.join(format!("{page}.jpg")), b"x").unwrap();
            }
        }

        let walked = walk(&root, 10).unwrap();

        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(walked.candidates.len(), 1);
        assert_eq!(walked.excluded, vec![3, 5]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli inventory::`
Expected: FAIL — `cannot find function 'volume_number' in this scope`.

- [ ] **Step 3: Implement the parse**

Insert into `crates/taiyaku-cli/src/inventory.rs` between the module comment and
`#[cfg(test)]`:

```rust
use std::path::{Path, PathBuf};

use crate::signature::Signature;

/// The volume number in a directory's name.
///
/// Takes the first digit run immediately preceded — optionally across one space
/// — by `v`, `vol`, `vol.` or `第`, and falls back to the first digit run when
/// none of those appear. Leading zeros are stripped, so `v007`, `v7` and `第7巻`
/// group together.
///
/// The marker is searched for before the fallback is used because a bare "first
/// digit run" rule reads `ONE PIECE v077 (2015)` correctly only by luck and a
/// "last digit run" rule reads it as 2015.
///
/// Deliberately not a regex. `regex` is three crates in a workspace whose short
/// dependency list `docs/status.md` states as a fact about the project, to do a
/// job a scan does; `--pattern` covers what the default misses. If a collection
/// turns up that neither handles, that is evidence for the dependency and it
/// can be added then.
pub fn volume_number(name: &str) -> Option<u32> {
    const MARKERS: [&str; 4] = ["vol.", "vol", "v", "第"];

    let chars: Vec<char> = name.chars().collect();
    let mut fallback = None;
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
        let number: Option<u32> = chars[start..at].iter().collect::<String>().parse().ok();
        if fallback.is_none() {
            fallback = number;
        }
        let before = chars[..start].iter().collect::<String>().to_lowercase();
        let before = before.strip_suffix(' ').unwrap_or(&before);
        if MARKERS.iter().any(|marker| before.ends_with(marker)) {
            return number;
        }
    }
    fallback
}

/// A `--pattern` literal with exactly one `{}` where the volume number goes.
#[derive(Debug, Clone)]
pub struct Pattern {
    before: String,
    after: String,
}

impl Pattern {
    pub fn parse(spec: &str) -> Result<Self, String> {
        let (before, after) = spec.split_once("{}").ok_or_else(|| {
            format!("--pattern `{spec}` needs one `{{}}` where the volume number goes")
        })?;
        if after.contains("{}") {
            return Err(format!(
                "--pattern `{spec}` has two `{{}}`; a name carries one volume number"
            ));
        }
        Ok(Self {
            before: before.to_string(),
            after: after.to_string(),
        })
    }

    /// The number where the pattern matches anywhere in the name.
    pub fn find(&self, name: &str) -> Option<u32> {
        let mut from = 0;
        while let Some(at) = name[from..].find(&self.before) {
            let start = from + at + self.before.len();
            let digits: String = name[start..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            let end = start + digits.len();
            if !digits.is_empty() && name[end..].starts_with(&self.after) {
                return digits.parse().ok();
            }
            // `before` may be empty (`--pattern '{}巻'`), so advance at least
            // one byte or this loop never terminates.
            from = from + at + self.before.len().max(1);
        }
        None
    }
}
```

- [ ] **Step 4: Implement the walk**

Add below the parse:

```rust
/// One directory that holds a whole edition.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub pages: u32,
}

/// What the walk found.
#[derive(Debug, Clone, Default)]
pub struct Walked {
    pub dirs: usize,
    pub candidates: Vec<Candidate>,
    /// Page counts of directories that held pages and fell short of the
    /// threshold, so the range they spanned can be printed. A threshold that
    /// nearly cut something real is then visible rather than inferred.
    pub excluded: Vec<u32>,
}

/// Directories holding a whole edition: the deepest one that still does.
///
/// A directory is a candidate when its recursive page count reaches
/// `min_pages` and none of its children reach it on their own. That handles the
/// nested case — `docs/sidecar.md` records seven of fifty-seven corpus archives
/// putting chapters in subdirectories — without making each chapter a copy.
///
/// **Its failure mode, named rather than hidden.** A directory holding nothing
/// but material below the threshold has no qualifying child, so it becomes one
/// enormous candidate. That is visible the instant it happens, because the page
/// count is printed and eight thousand pages does not look like a volume.
pub fn walk(root: &Path, min_pages: u32) -> Result<Walked, String> {
    let mut walked = Walked::default();
    visit(root, min_pages, &mut walked)?;
    walked.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    walked.excluded.sort_unstable();
    Ok(walked)
}

fn visit(dir: &Path, min_pages: u32, walked: &mut Walked) -> Result<u32, String> {
    walked.dirs += 1;
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut here, mut children) = (0u32, Vec::new());
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            children.push(path);
        } else if taiyaku_read::is_page_name(&name) {
            here += 1;
        }
    }

    children.sort();
    let mut total = here;
    let mut child_qualifies = false;
    for child in children {
        let pages = visit(&child, min_pages, walked)?;
        total += pages;
        child_qualifies |= pages >= min_pages;
    }

    // A directory holding no pages anywhere is a container, neither a candidate
    // nor an exclusion — counting it as excluded would fill the reported range
    // with zeroes and hide what actually fell short.
    if !child_qualifies && total > 0 {
        match total >= min_pages {
            true => walked.candidates.push(Candidate {
                path: dir.to_path_buf(),
                pages: total,
            }),
            false => walked.excluded.push(total),
        }
    }
    Ok(total)
}
```

- [ ] **Step 5: Implement the command**

Add below the walk:

```rust
/// Above the largest thing the One Piece run excluded (28 pages) and well below
/// the smallest real volume in the corpus (v092's all-spread copy at 101).
const DEFAULT_MIN_PAGES: u32 = 60;

pub fn command(args: &[String]) -> Result<(), String> {
    let root = PathBuf::from(
        args.first()
            .filter(|a| !a.starts_with("--"))
            .ok_or("needs a directory to walk")?,
    );
    let min_pages = match crate::option_value(args, "--min-pages") {
        Some(given) => given
            .parse::<u32>()
            .map_err(|_| format!("--min-pages `{given}` is not a number of pages"))?,
        None => DEFAULT_MIN_PAGES,
    };
    let pattern = match crate::option_value(args, "--pattern") {
        Some(spec) => Some(Pattern::parse(spec)?),
        None => None,
    };

    let walked = walk(&root, min_pages)?;
    let range = match (walked.excluded.first(), walked.excluded.last()) {
        (Some(low), Some(high)) => format!(" ({low}-{high} pages)"),
        _ => String::new(),
    };
    eprintln!(
        "walked {} directories | {} candidates | excluded {}{range}",
        walked.dirs,
        walked.candidates.len(),
        walked.excluded.len()
    );

    // Grouped by parsed number, `None` last, so a candidate whose name carries
    // no number is reported rather than dropped.
    let mut groups: Vec<(Option<u32>, Vec<&Candidate>)> = Vec::new();
    for candidate in &walked.candidates {
        let name = candidate.path.file_name().unwrap_or_default().to_string_lossy();
        let number = match &pattern {
            Some(pattern) => pattern.find(&name),
            None => volume_number(&name),
        };
        match groups.iter_mut().find(|(volume, _)| *volume == number) {
            Some((_, copies)) => copies.push(candidate),
            None => groups.push((number, vec![candidate])),
        }
    }
    groups.sort_by_key(|(volume, _)| volume.unwrap_or(u32::MAX));

    eprintln!("\nvol  copies  signature   paths");
    let (mut duplicated, mut settled) = (0, 0);
    for (volume, copies) in &groups {
        if copies.len() < 2 {
            continue;
        }
        duplicated += 1;
        let signatures: Vec<Signature> = copies
            .iter()
            .map(|c| Signature::of(&[c.path.clone()]))
            .collect::<Result<_, _>>()?;
        let largest = signatures
            .iter()
            .map(|one| signatures.iter().filter(|s| s.digest == one.digest).count())
            .max()
            .unwrap_or(1);
        if largest == copies.len() {
            settled += 1;
        }
        let column = match largest {
            1 => "distinct".to_string(),
            n if n == copies.len() => "identical".to_string(),
            n => format!("{n} of {}", copies.len()),
        };
        let paths: Vec<String> = copies
            .iter()
            .map(|c| c.path.to_string_lossy().to_string())
            .collect();
        eprintln!(
            "{:<4} {:<7} {column:<11} {}",
            match volume {
                Some(number) => format!("{number:03}"),
                None => "?".to_string(),
            },
            copies.len(),
            paths.join(", ")
        );
    }

    println!(
        "inventory: dirs={} candidates={} volumes={} duplicated={duplicated} settled={settled} \
         excluded={} excluded-pages={}",
        walked.dirs,
        walked.candidates.len(),
        groups.len(),
        walked.excluded.len(),
        match (walked.excluded.first(), walked.excluded.last()) {
            (Some(low), Some(high)) => format!("{low}-{high}"),
            _ => "-".to_string(),
        }
    );
    Ok(())
}
```

Register and dispatch in `crates/taiyaku-cli/src/main.rs` — `mod inventory;` in
the module list, then in `run`:

```rust
        Some("inventory") => inventory::command(&args[1..]),
```

And in the usage block:

```rust
            println!("       taiyaku inventory <root> [--min-pages N] [--pattern SPEC]");
```

- [ ] **Step 6: Run everything**

Run: `cargo test -p taiyaku-cli inventory::`
Expected: PASS, 8 tests.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no output, exit 0.

Run it for real:

```bash
cargo run -p taiyaku-cli -- inventory corpus/incoming/op-duplicates
```
Expected: 16 rows, 34 copies, and an `inventory:` line reading
`volumes=16 duplicated=16`.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-cli/src/inventory.rs crates/taiyaku-cli/src/main.rs
git commit -m "Find the duplicate sets in a release, and show what was guessed

status.md §2 states three steps and leaves out the one that came before them:
getting from 579 directories to the 19 sets worth comparing. Both decisions in
that step are guesses -- a page-count threshold and a volume number parsed from
a name -- so they live in their own command and print what they decided.

A candidate is the deepest directory that still holds a whole edition, which
handles the nested case sidecar.md records without making each chapter a copy.
Its failure mode is named: a folder of nothing but Jump issues becomes one
enormous candidate, which is visible because the page count is printed. The
threshold defaults to 60, above the largest thing the One Piece run excluded at
28 and well below the corpus's smallest real volume at 101.

No regex. The default marker scan and a one-placeholder --pattern cover every
name in notes.md, and regex would be three crates for it."
```

---

### Task 7: the five gated corpus checks and the survey script

**Files:**
- Modify: `crates/taiyaku-cli/src/corpus.rs`
- Create: `scripts/duplicate-survey.sh`

**Interfaces:**
- Consumes: `duplicates::{resolve, set_line, Relation}`, `inventory::walk`.
- Produces: nothing other tasks use.

These run only with `TAIYAKU_CORPUS` set and print a skip line otherwise, the
pattern `corpus.rs` already establishes at lines 40-43 and 75-78.

- [ ] **Step 1: Write the checks**

Append to `crates/taiyaku-cli/src/corpus.rs`:

```rust
/// The cache the duplicate checks share with `align`, beside the corpus rather
/// than in the working directory — which for `cargo test -p` is the crate
/// directory. Same reasoning as the star check above.
fn duplicate_cache(root: &Path) -> PathBuf {
    root.parent()
        .unwrap_or(Path::new("."))
        .join(".taiyaku-cache")
}

fn copies(root: &Path, under: &str, names: &[&str]) -> Vec<Vec<PathBuf>> {
    names
        .iter()
        .map(|name| vec![root.join("incoming").join(under).join(name)])
        .collect()
}

/// One Piece volume 77: the case that argues for all pairs rather than every
/// copy against the first.
///
/// c0 and c1 hold 234 images each and c2 holds 214 — and it is c1 and c2 that
/// are one edition, the smaller being all-spread and expanding to the same 234
/// slots. Anchoring on c0 reported three mutually unrelated files, and c0 is
/// exactly the copy most likely to be the odd one out because the sort that
/// produced it was by page count.
///
/// This check goes red if step 2 is ever rewritten as against-copy-0.
#[test]
fn duplicates_finds_the_identity_hidden_behind_the_first_copy() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "op-duplicates",
        &["v077-mono-c0", "v077-mono-c1", "v077-mono-c2"],
    );

    let resolved = crate::duplicates::resolve(
        &set,
        None,
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("v077 resolves");

    eprintln!("v077: {}", crate::duplicates::set_line(&resolved));
    let merged: Vec<&Vec<usize>> = resolved.clusters.iter().filter(|c| c.len() > 1).collect();
    assert_eq!(merged.len(), 1, "exactly one identity cluster: {:?}", resolved.clusters);
    assert_eq!(merged[0].len(), 2);
    assert!(
        merged[0].contains(&2),
        "the 214-image copy is one of the pair: {:?}",
        merged[0]
    );
    assert!(resolved.contradictions.is_empty());
    assert!(resolved.flagged.is_empty(), "all three are volume 77");

    let identity = resolved
        .pairs
        .iter()
        .find(|p| p.relation == crate::duplicates::Relation::Identity)
        .expect("an identity pair");
    assert_eq!(
        identity.runs, 1,
        "full coverage on both sides is one run, or run compression is broken"
    );
}

/// Identity across a completely different pagination: 200 images against 101,
/// the smaller being an all-spread edition. Goes red if slot expansion stops
/// being load-bearing here.
#[test]
fn duplicates_sees_through_a_different_pagination() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(&root, "op-duplicates", &["v092-mono-c0", "v092-mono-c1"]);

    let resolved = crate::duplicates::resolve(
        &set,
        None,
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("v092 resolves");

    eprintln!("v092: {}", crate::duplicates::set_line(&resolved));
    assert_eq!(resolved.clusters, vec![vec![0, 1]], "one edition, two packings");
    assert_eq!(resolved.pairs[0].paired, 200);
    assert_eq!(resolved.pairs[0].runs, 1);
}

/// The Bocchi file named as main-series volume 1 that is the Gaiden spinoff.
///
/// What is asserted is the flagging: it refuses against both real scans, is kept
/// out of the ranking, and the survivors rank by slots. What is **not** asserted
/// is whether the two real scans cluster as one — `notes.md` calls them "the
/// identity case again" at 124 and 125 pages, which is the same shape as the
/// Chainsaw Man pair the same document keeps distinct, so the two readings
/// disagree and only slot counts settle it. Printed here; pin the observed
/// value once this has run.
#[test]
fn duplicates_flags_a_file_that_is_not_the_volume_it_claims() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "bocchi",
        &["raw-v01-src-hamaji", "raw-v01-src-krc", "raw-v01-src-mangazip"],
    );

    let resolved = crate::duplicates::resolve(
        &set,
        None,
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("bocchi resolves");

    eprintln!("bocchi: {}", crate::duplicates::set_line(&resolved));
    eprintln!(
        "bocchi slots: {:?} | clusters {:?}",
        resolved.copies.iter().map(|c| c.slots).collect::<Vec<_>>(),
        resolved.clusters
    );
    assert_eq!(resolved.flagged, vec![2], "the Gaiden file, and only it");
    assert!(
        resolved.ranked.iter().all(|r| !r.cluster.contains(&2)),
        "a flagged copy must not be ranked"
    );
    let places: Vec<usize> = resolved.ranked.iter().map(|r| r.slots).collect();
    assert!(
        places.windows(2).all(|w| w[0] >= w[1]),
        "ranked by slots descending: {places:?}"
    );
}

/// Base selection against a reference edition: four Chainsaw Man raws against
/// the English release. `src-b` holds 199 images to the others' 198 and wins on
/// content.
///
/// Not asserted: which of the two 198-image scans pair as an identity cluster.
/// `notes.md` says two of the three are the same pagination, and the 198-image
/// pair is the candidate, but the mapping from that document's names to the
/// corpus's is an inference from page counts. Printed; pin it once run.
#[test]
fn duplicates_ranks_scans_against_a_reference_edition() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "chainsawman",
        &["raw-v01-src-a", "raw-v01-src-b", "raw-v01-src-c", "raw-v01-colour"],
    );
    let against = vec![root.join("incoming/chainsawman/en-v01.cbz")];

    let resolved = crate::duplicates::resolve(
        &set,
        Some(&against),
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("chainsawman resolves");

    eprintln!("chainsawman: {}", crate::duplicates::set_line(&resolved));
    eprintln!("chainsawman clusters: {:?}", resolved.clusters);
    let winner = &resolved.ranked[0];
    assert!(
        winner.cluster.contains(&1),
        "the 199-image scan wins on content: {:?}",
        resolved.ranked
    );
    assert!(
        winner.against.is_some(),
        "a reference edition was given, so every row carries its figures"
    );
}

/// The inventory over the duplicate corpus: every set found, and the parse
/// surviving real names.
#[test]
fn the_inventory_finds_every_duplicate_set() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let under = root.join("incoming/op-duplicates");

    let walked = crate::inventory::walk(&under, 60).expect("the corpus walks");

    eprintln!(
        "op-duplicates: {} candidates, excluded {:?}",
        walked.candidates.len(),
        walked.excluded
    );
    assert_eq!(walked.candidates.len(), 34, "34 copies");

    let mut volumes: Vec<u32> = walked
        .candidates
        .iter()
        .map(|c| {
            let name = c.path.file_name().unwrap_or_default().to_string_lossy().to_string();
            crate::inventory::volume_number(&name)
                .unwrap_or_else(|| panic!("no volume number parsed from `{name}`"))
        })
        .collect();
    volumes.sort_unstable();
    volumes.dedup();
    assert_eq!(volumes.len(), 16, "16 volumes: {volumes:?}");
    assert!(volumes.contains(&95), "v095-colour-c0 parses to 95");
}
```

`corpus.rs` needs `use std::path::PathBuf;` — it already imports
`std::path::{Path, PathBuf}` at line 20.

- [ ] **Step 2: Run them without the corpus**

Run: `cargo test -p taiyaku-cli corpus:: -- --nocapture`
Expected: PASS, every check printing `TAIYAKU_CORPUS unset; skipping`.

- [ ] **Step 3: Run them against the corpus**

Run:
```bash
TAIYAKU_CORPUS="$(cd corpus && pwd -P)" cargo test -p taiyaku-cli corpus:: -- --nocapture --test-threads=1
```
Expected: PASS. The first run fingerprints ~1,900 pages and takes minutes; later
runs are milliseconds from `.taiyaku-cache`.

**Record what the two undecided cases came out as.** The Bocchi check prints its
slot counts and clusters; the Chainsaw Man check prints its clusters. Write both
into `docs/notes.md` in Task 8, and tighten the two checks to assert the
observed cluster shape — the value is measured first and pinned second, which is
how every other number in this repository was arrived at.

- [ ] **Step 4: Write the survey script**

Create `scripts/duplicate-survey.sh`:

```bash
#!/usr/bin/env bash
# Collect one `set:` line per duplicate set, for a table in docs/notes.md.
#
#   scripts/duplicate-survey.sh corpus/incoming/op-duplicates
#
# Sets are formed by volume number, the same way `taiyaku inventory` forms them.
# The point of this file is that the table in notes.md can be rebuilt rather
# than retyped: the same argument produces the same rows.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

root="${1:?usage: duplicate-survey.sh <root>}"
[ -d "$root" ] || { echo "error: $root is not a directory" >&2; exit 2; }

taiyaku=(cargo run --quiet --release -p taiyaku-cli --)

# Group by the volume number in each directory's name, which is what inventory
# groups by. Anything with fewer than two copies is not a duplicate set.
volumes="$(find "$root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' \
  | sed -n 's/.*[vV]\([0-9][0-9]*\).*/\1/p' | sort -u)"

for volume in $volumes; do
  mapfile -t set < <(find "$root" -mindepth 1 -maxdepth 1 -type d -name "*${volume}*" | sort)
  [ "${#set[@]}" -ge 2 ] || continue

  args=()
  for copy in "${set[@]}"; do
    [ "${#args[@]}" -eq 0 ] || args+=("--")
    args+=("$copy")
  done

  echo "== v${volume} (${#set[@]} copies)" >&2
  "${taiyaku[@]}" duplicates "${args[@]}" 2>/dev/null
done
```

Make it executable: `chmod +x scripts/duplicate-survey.sh`.

- [ ] **Step 5: Run the survey**

Run: `scripts/duplicate-survey.sh corpus/incoming/op-duplicates`
Expected: one `set:` line per volume with two or more copies, on stdout, and a
`== vNNN` header per set on stderr. Keep the output — Task 8 puts it in
`notes.md`.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/corpus.rs scripts/duplicate-survey.sh
git commit -m "Measure duplicate resolution against the corpus, five ways

One check per claim status.md §2 makes, plus a second One Piece volume because
v077 and v092 measure different things and folding them into one test would
report a single failure for two unrelated regressions.

v077 is the check that goes red if all-pairs is ever rewritten as
against-copy-0, which is the mistake notes.md records making. Two checks
deliberately do not assert everything they print: whether Bocchi's 124- and
125-page scans cluster as one, and which of Chainsaw Man's two 198-image scans
pair. notes.md and the identity definition disagree about the first and the
second is an inference from page counts, so both get measured before they get
pinned."
```

---

### Task 8: documentation

**Files:**
- Modify: `docs/status.md:523-538` (§2) and `:67-98` (the command list)
- Modify: `docs/notes.md`
- Modify: `corpus/README.md`
- Modify: `scripts/check-all.sh` if the new checks need naming there (they do
  not — it runs `corpus::` wholesale)

- [ ] **Step 1: Pin the two measured-not-asserted outcomes**

From the Task 7 run, take the printed Bocchi slot counts and clusters and the
Chainsaw Man clusters. Tighten both checks in `crates/taiyaku-cli/src/corpus.rs`
to assert the observed cluster shape, replacing the "printed; pin once run"
sentence in each doc comment with what was actually measured.

- [ ] **Step 2: Update `docs/status.md`**

In the command list at lines 67-75, add the two commands:

```
taiyaku inventory <root>                        [--min-pages N] [--pattern SPEC]
taiyaku duplicates <copy>... -- <copy>...       [--against <member>...] [--ltr]
```

Change the sentence at line 67 from `Five commands:` to `Seven commands:`.

Add a paragraph after the `--stats` paragraph (line 95-98) describing what the
two do, in the register of the surrounding prose: what each takes, what it
prints, and that neither moves a file.

Replace §2 (lines 523-538) with a landed section. It should say what shipped,
what the corpus checks measured, and — following the M3 precedent of "Say what
shipped, not what the design sketched" — name the one place the protocol as
written and the protocol as built differ: `notes.md` used "identity" two ways
and the command uses only the stricter one.

Update the crate table at lines 54-59 with `signature`, `duplicates` and
`inventory` under `taiyaku-cli`, and the test count at line 25 (`201 tests`) to
whatever `cargo test --workspace 2>&1 | grep 'test result'` now reports summed.

- [ ] **Step 3: Update `docs/notes.md`**

Add a section after "Sorting 109 volumes' worth of duplicates" recording:

- The five corpus checks and what each measured, with the numbers from the
  Task 7 run.
- The two outcomes this design declined to predict, and what they came out as.
- The `notes.md` internal discrepancy, stated plainly: the Bocchi section calls
  a 124/125 pair "the identity case again" while the Chainsaw Man section keeps
  the same shape distinct, and the command follows the stricter reading. Correct
  the Bocchi wording in place rather than leaving both readings standing.
- The `set:` line table from `scripts/duplicate-survey.sh`, so the One Piece run
  is reproducible rather than remembered.

- [ ] **Step 4: Update `corpus/README.md`**

Add to the "Running things against it" section, beside the `align` invocations:

```sh
# the identity case that a file listing gets wrong: 200 images against 101
taiyaku duplicates corpus/incoming/op-duplicates/v092-mono-c0 \
  -- corpus/incoming/op-duplicates/v092-mono-c1

# all pairs, not everything against the first copy: c1 and c2 are one edition
taiyaku duplicates corpus/incoming/op-duplicates/v077-mono-c{0,1,2}

# base selection against a reference: src-b wins on its 199th page
taiyaku duplicates corpus/incoming/chainsawman/raw-v01-src-{a,b,c} \
  -- corpus/incoming/chainsawman/raw-v01-colour \
  --against corpus/incoming/chainsawman/en-v01.cbz
```

Note that the second invocation's brace expansion produces three arguments with
no `--` between them, which is one copy of three members and not three copies —
write it out with the separators.

Add a row to the expectations table:

| One Piece v077 duplicates | **c1 ≡ c2, c0 distinct** | all-pairs collapsed back to against-first |

- [ ] **Step 5: Verify the whole gate**

Run: `scripts/check-all.sh`
Expected: `all green`.

Run: `TAIYAKU_CORPUS="$(cd corpus && pwd -P)" scripts/check-all.sh`
Expected: `all green`, with the five new checks printing their measurements
under the `==> corpus:` heading.

- [ ] **Step 6: Commit**

```bash
git add docs/status.md docs/notes.md corpus/README.md crates/taiyaku-cli/src/corpus.rs
git commit -m "Record what duplicate resolution measured, and fix a word notes.md used twice

status.md §2 becomes a landed section: two commands, five gated checks, and the
numbers each one measured. corpus/README.md gains the three invocations beside
the align ones it already lists.

notes.md used identity two ways -- the Bocchi section calls a 124-and-125-page
pair the identity case while the Chainsaw Man section keeps the same shape
distinct and ranks the larger one first. The stricter reading is the one that
makes base selection work, so the Bocchi wording is corrected rather than left
standing beside a definition that contradicts it."
```

---

## Self-review

**Spec coverage.** Every section of the design maps to a task: the two command
shapes (5, 6), argument parsing (5), the candidate rule and `--min-pages` (6),
the name parse and `--pattern` and the no-regex decision (6), signature over
uncompressed page sizes (1, 2), confirmation by content hash (2, 5), identity
and its one-run consequence (3), one solve per unordered pair (5), spreads
always splitting (5), union-find and non-transitivity (3), refusal flagging
including the two-copy case (3), ranking and ties and the representative rule
(3, 5), both output forms and the field order (4), code placement (all), the
five corpus checks (7), the unit tests (1-6), and the docs (8).

**Two things the spec did not foresee, resolved here.**
`taiyaku_read::member_digest` cannot serve as the confirmation hash — it hashes
an archive's file bytes and a directory's page bytes, so one scan filed two ways
hashes differently, which is the case a signature match is most likely
reporting. Task 2 adds `content_digest` over page bytes instead. And
`taiyaku-cli` has no `sha2` dependency; Task 2 adds the edge, which changes no
documented dependency because `taiyaku-read` already pulls the crate and
`status.md` already names it.

**Type consistency.** `Pair::judge` takes `(a, b, runs, a_slots, b_slots)` in
Tasks 3, 4 and 5. `Ranked` carries `cluster: Vec<usize>` throughout, and
`ranked[n].cluster[0]` is the representative in Tasks 4, 5 and 7. `Against` is
`{ runs: usize, paired: u32 }` in Tasks 3, 4 and 5, matching
`paired_slots -> u32` and `Vec::len -> usize`. `walk` returns `Walked` in Tasks
6 and 7. `Signature::of` takes `&[PathBuf]` in Tasks 2, 5 and 6.

# The library's download, server half — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `GET /api/works/{slug}/{edition}/download` streams an edition as one
`STORE` CBZ with an exact `Content-Length` and working ranges, and
`.../members/{ordinal}/download` serves one member's original bytes — both
behind the root grant and `can_download`.

**Architecture:** Two new modules in `taiyaku-serve`. `cbz.rs` is pure
arithmetic and bytes: given a list of `(name, size, crc32)` it computes the
complete output layout and can emit the file from any byte offset. `download.rs`
is the HTTP layer: it decides passthrough versus generated, builds the plan,
runs the byte production on a blocking thread feeding a bounded channel, and
sets the headers. `taiyaku-read` gains two functions — one that describes a
member's pages without reading them, one that streams them with a single archive
open. Nothing caches; nothing is buffered whole.

**Tech Stack:** Rust 2024 (rust-version 1.85), `axum` 0.8, `zip` 7.2 (read side
and tests only — the writer is hand-rolled so the layout is exactly known),
`crc32fast` 1.5, `tokio` `spawn_blocking` + `mpsc`, `futures-core` for the one
`Stream` impl.

**Spec:** `docs/superpowers/specs/2026-08-23-library-download-design.md`. Read
§1 through §7 and §10 before starting; this plan implements §2–§7 and §10 and
leaves §8 (the tile control) and §9 (OPDS) to the second plan.

## Global Constraints

- **Squash-only remote.** One branch is one commit on `main`. Branch is
  `library-download`, already created, already holding the spec commit.
- **No new workspace dependency that is not already in `Cargo.lock`.** Three
  are added as *direct* deps of crates that already get them transitively —
  `crc32fast = "1.5.0"` (taiyaku-read), `futures-core = "0.3.33"` and
  `bytes = "1.12.1"` (taiyaku-serve) — following the precedent
  `taiyaku-serve/Cargo.toml` already records for `rusqlite`: named directly so
  the code may name the type, same version so the resolver unifies one copy.
  `tokio`'s feature list gains `"sync"` for `mpsc`.
- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.** Never run
  `check-all.sh` with `TAIYAKU_CORPUS` set — it exports it into an unfiltered
  `cargo test --workspace`, which runs `geometry_experiment` in debug (a run
  was killed at 1h09m). The corpus job runs separately:
  `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`.
- **Verify the gate by its own words.** Quote the `all green` line. Piping
  through `tail` or appending `; echo` masks the exit status.
- **Every task ends with `cargo fmt --all`**, not only `cargo test` and
  `cargo clippy`. *This line was missing for the branch's first seven tasks and
  the omission cost: fifty `rustfmt` violations accumulated silently, because
  `-D warnings` says nothing about formatting and no per-task gate ran
  `cargo fmt`. `check-all.sh:149` runs `cargo fmt --all -- --check`, so the
  whole lot would have failed at Task 11 with the branch's authorship of it no
  longer obvious. A reviewer that did notice concluded it was pre-existing
  drift, having compared against a commit on this branch rather than against
  `main` — `main` is clean.*
- **The binding clippy gate is `-D warnings` on the DEFAULT lints**
  (`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `scripts/check-all.sh:150`). `clippy::pedantic` runs as a **separate advisory
  pass** that ends in `|| true` (`:188`), so a pedantic warning does not fail
  the build. *This line said "clippy is pedantic and warnings are denied" and
  was wrong; Task 1's implementer caught it.* Avoid pedantic warnings anyway,
  but the lints that will actually stop you are the default ones — Task 1 was
  stopped by `type_complexity`, not by a cast.
- **`#[expect(...)]` over `#[allow(...)]`** — it fails the build when the lint
  stops firing. And prefer neither: when `type_complexity` fires on a
  `&mut dyn FnMut(...)` parameter, **name the type** rather than silence the
  lint. The house precedent is `taiyaku-cli/src/duplicates.rs:26`
  (`type Fingerprint<'a> = dyn FnMut(...) -> Result<_, String> + 'a`), and Task
  1 followed it with `type PageVisitor<'a>` in `read/pages.rs:681`. Later tasks
  taking closures should do the same.
- **Every route goes in `routes::guarded_routes`**, never `open_routes`.
  `auth::tests::every_route_refuses_an_anonymous_request` derives its count
  from both lists, so a new route needs that count updated in the same commit.
- **No figure in a doc comment that was not measured.** If a comment states a
  cost, it names where the measurement is.
- **`docs/notes.md` takes the measurements, `docs/status.md` takes the state.**
  Task 11 does both, and the open-items sweep is part of it.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-read/Cargo.toml` | + `crc32fast = "1.5.0"` |
| `crates/taiyaku-read/src/pages.rs` | + `PageEntry`, `member_page_entries`, `for_each_member_page`. The three existing listing functions are **untouched** — see the plan's Task 1 note and spec §4 |
| `crates/taiyaku-read/src/lib.rs` | + two names in the re-export list |
| `crates/taiyaku-read/src/tests.rs` | Task 1's tests |
| `crates/taiyaku-serve/Cargo.toml` | + `bytes`, `futures-core`; `tokio` gains `"sync"` |
| `crates/taiyaku-serve/src/cbz.rs` **(new)** | The layout: `PlannedEntry`, `Plan`, `Plan::of`, `Plan::len`, `Plan::emit_from`. Pure — no I/O, no HTTP, no library rows |
| `crates/taiyaku-serve/src/cbz/tests.rs` **(new)** | Round trip through the `zip` reader; whole-versus-parts; zip64 arithmetic |
| `crates/taiyaku-serve/src/comicinfo.rs` **(new)** | `ComicInfo` XML from library rows. Pure given a struct; one query function beside it |
| `crates/taiyaku-serve/src/comicinfo/tests.rs` **(new)** | Element presence and absence, escaping |
| `crates/taiyaku-serve/src/download.rs` **(new)** | The two handlers, passthrough/generated choice, the blocking producer + channel body, `Range`, `Content-Disposition`, the download filename |
| `crates/taiyaku-serve/src/download/tests.rs` **(new)** | Route-level tests through `oneshot` |
| `crates/taiyaku-serve/src/lib.rs` | `mod cbz; mod comicinfo; pub(crate) mod download;` |
| `crates/taiyaku-serve/src/routes.rs` | `refuse_if_cannot_download`, `refuse_if_cannot_stream`; three route registrations; `get_page` gains the stream guard |
| `crates/taiyaku-serve/src/opds.rs` | `stream_page` gains the stream guard. **The acquisition link is the second plan's, not this one's** |
| `crates/taiyaku-serve/src/auth/tests.rs` | the enumeration count |
| `crates/taiyaku-serve/src/corpus.rs` | two standing checks |
| `docs/notes.md`, `docs/status.md` | Task 11 |

---

### Task 1: `taiyaku-read` can describe and stream a member's pages

**Files:**
- Modify: `crates/taiyaku-read/Cargo.toml`
- Modify: `crates/taiyaku-read/src/pages.rs` (add after `member_page_sizes`, ~line 600)
- Modify: `crates/taiyaku-read/src/lib.rs:17-18` (the re-export list)
- Test: `crates/taiyaku-read/src/tests.rs`

**Interfaces:**
- Consumes: the existing private helpers `directory_page_paths`, `page_names_from`, `open_archive`, and `MAX_ENTRY_CAPACITY_HINT`.
- Produces:
  - `pub struct PageEntry { pub name: String, pub size: u64, pub crc32: u32 }`, deriving `Debug, Clone, PartialEq, Eq`
  - `pub fn member_page_entries(path: &Path) -> Result<Vec<PageEntry>, String>`
  - `pub fn for_each_member_page(path: &Path, f: &mut dyn FnMut(&PageEntry, &[u8]) -> Result<(), String>) -> Result<(), String>`

**Do not fold `member_page_names` or `member_page_sizes` onto the new
function.** It looks like removing a duplicate traversal and is a regression:
`member_pages` is `member_page_names().len()` and is on the scanner's and
`align`'s path, so a directory member would start hashing every page just to be
counted; and `member_page_sizes`'s own doc comment states the property that
would go — *"A listing, not a read — the property that lets an inventory run
over a NAS and decide what is worth copying"* — with
`taiyaku-cli/src/signature.rs:51` as the caller relying on it. Spec §4 records
this. Leave all three functions as they are.

- [ ] **Step 1: Add the dependency**

In `crates/taiyaku-read/Cargo.toml`, under `[dependencies]`, keeping the list
alphabetical:

```toml
# Already in the dependency tree via `zip` — named directly here only so
# `member_page_entries` can hash a directory member's pages, which is the one
# case a zip central directory cannot answer. Same version `zip` resolves to,
# so the resolver unifies one copy rather than two.
crc32fast = "1.5.0"
sha2 = "0.11.0"
zip = "7.2.0"
```

- [ ] **Step 2: Write the failing tests**

Append to `crates/taiyaku-read/src/tests.rs`. The file already has helpers for
building fixture archives — read the top of it and reuse whatever it has for
writing a temp zip rather than adding a second way to do it. If it has none,
these three are self-contained:

```rust
#[test]
fn member_page_entries_reads_names_sizes_and_crcs_from_an_archive() {
    let dir = std::env::temp_dir().join(format!("entries-archive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.cbz");
    {
        use std::io::Write as _;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        // Deflated deliberately: 97.9% of this corpus's entries are, and the
        // point of this function is that it never inflates one.
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in [("002.jpg", &b"second page"[..]), ("001.jpg", &b"first"[..])] {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    let entries = crate::member_page_entries(&path).unwrap();

    // Natural order, not stored order — the archive above was written 002 first.
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["001.jpg", "002.jpg"]
    );
    assert_eq!(entries[0].size, 5);
    assert_eq!(entries[1].size, 11);
    assert_eq!(entries[0].crc32, crc32fast::hash(b"first"));
    assert_eq!(entries[1].crc32, crc32fast::hash(b"second page"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn member_page_entries_hashes_a_directory_members_pages() {
    let dir = std::env::temp_dir().join(format!("entries-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("member/ch1")).unwrap();
    std::fs::write(dir.join("member/ch1/002.jpg"), b"two").unwrap();
    std::fs::write(dir.join("member/ch1/001.jpg"), b"one").unwrap();
    // Not a page, and must not become an entry.
    std::fs::write(dir.join("member/ComicInfo.xml"), b"<x/>").unwrap();

    let entries = crate::member_page_entries(&dir.join("member")).unwrap();

    // The whole path relative to the member, not the basename — `docs/sidecar.md`.
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["ch1/001.jpg", "ch1/002.jpg"]
    );
    assert_eq!(entries[0].crc32, crc32fast::hash(b"one"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn for_each_member_page_visits_every_page_in_order_with_its_entry() {
    let dir = std::env::temp_dir().join(format!("each-page-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.cbz");
    {
        use std::io::Write as _;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in [("001.jpg", &b"one"[..]), ("002.jpg", &b"two"[..])] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    let mut seen: Vec<(String, u64, Vec<u8>)> = Vec::new();
    crate::for_each_member_page(&path, &mut |entry, bytes| {
        seen.push((entry.name.clone(), entry.size, bytes.to_vec()));
        Ok(())
    })
    .unwrap();

    assert_eq!(
        seen,
        vec![
            ("001.jpg".to_string(), 3, b"one".to_vec()),
            ("002.jpg".to_string(), 3, b"two".to_vec()),
        ]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn for_each_member_page_stops_at_the_first_refusal() {
    let dir = std::env::temp_dir().join(format!("each-stop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.cbz");
    {
        use std::io::Write as _;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for name in ["001.jpg", "002.jpg", "003.jpg"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
    }

    let mut count = 0usize;
    let refused = crate::for_each_member_page(&path, &mut |_, _| {
        count += 1;
        if count == 2 { Err("the caller gave up".to_string()) } else { Ok(()) }
    });

    assert_eq!(refused, Err("the caller gave up".to_string()));
    // The third page was never read: a download whose client hung up must not
    // keep inflating the rest of a 1.49 GB volume.
    assert_eq!(count, 2);

    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p taiyaku-read member_page_entries for_each_member_page`
Expected: FAIL — `cannot find function 'member_page_entries' in crate 'crate'`
and the same for `for_each_member_page`.

- [ ] **Step 4: Implement both functions**

Append to `crates/taiyaku-read/src/pages.rs`, after `member_page_sizes` and
before `member_entry`:

```rust
/// One page entry, described without being decoded.
///
/// The triple a `STORE` zip's local header needs, and the reason this exists
/// beside `member_page_names` and `member_page_sizes` rather than replacing
/// them: those two are listings, and this one hashes. For an **archive** all
/// three fields come out of the central directory, so the archive's page data
/// is never touched; for a **directory** the crc does not exist anywhere and
/// each page is read once to compute it. Spec C2 §4 records why paying that on
/// directory members beat the alternative (data descriptors) and why folding
/// the listings onto this would have been a regression, not a de-duplication.
///
/// `size` is the **uncompressed** size on both sides, which is what a `STORE`
/// output writes and what makes a directory of loose images and a `.cbz` of the
/// same scan produce the same list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEntry {
    pub name: String,
    pub size: u64,
    pub crc32: u32,
}

/// Every page of a member, described in reading order.
///
/// The name is the same one `member_page_names` reports — whole path relative
/// to the member for a directory, the bare entry name for an archive — and an
/// entry name that is not valid UTF-8 arrives garbled rather than refused, for
/// the reason that function's doc comment gives.
///
/// # Errors
///
/// For a directory, returns an error if it, or a directory nested under it,
/// cannot be listed, or if a page cannot be read back — removed or made
/// unreadable since listing. For an archive, returns an error if it cannot be
/// opened, if its leading bytes identify a recognised-but-unsupported
/// container — 7-Zip, RAR, or tar, named as such — if it does not parse as a
/// zip archive, or if a listed entry cannot be looked up again by name.
pub fn member_page_entries(path: &Path) -> Result<Vec<PageEntry>, String> {
    if path.is_dir() {
        return directory_page_paths(path)?
            .iter()
            .map(|page| {
                let name = page
                    .strip_prefix(path)
                    .unwrap_or(page)
                    .to_string_lossy()
                    .to_string();
                // Read rather than `metadata` + read: the size that goes into
                // the header and the bytes that were hashed then provably come
                // from the same read, where a stat first would leave a window
                // in which the file changed size between the two.
                let bytes = std::fs::read(page).map_err(|e| format!("{}: {e}", page.display()))?;
                Ok(PageEntry {
                    name,
                    size: bytes.len() as u64,
                    crc32: crc32fast::hash(&bytes),
                })
            })
            .collect();
    }
    let mut zip = open_archive(path)?;
    let names = page_names_from(&mut zip);
    names
        .iter()
        .map(|name| {
            let entry = zip
                .by_name(name)
                .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
            Ok(PageEntry {
                name: name.clone(),
                size: entry.size(),
                crc32: entry.crc32(),
            })
        })
        .collect()
}

/// Every page of a member, handed to `f` one at a time, in reading order.
///
/// **One archive open for the whole member, one page in memory at a time.**
/// `member_page` opens the archive per index, which is right for a route
/// serving one page and wrong for a download reading two hundred of them: this
/// is that loop with the open hoisted out. `read_edition` is the other
/// neighbour and holds *every* page at once, which for this corpus's largest
/// member would be 1.49 GB.
///
/// The `PageEntry` travels with the bytes so a caller that planned an output
/// from `member_page_entries` can check the member still matches as it streams
/// — an archive replaced mid-download is then detectable rather than producing
/// a file whose central directory lies about its own contents.
///
/// **`f`'s `Err` stops the walk on the spot** and is returned unchanged. A
/// client that hung up must not cost the rest of the volume.
///
/// # Errors
///
/// The errors of [`member_page_entries`], plus whatever `f` returns, plus — for
/// an archive — an entry whose compressed data does not decode.
pub fn for_each_member_page(
    path: &Path,
    f: &mut dyn FnMut(&PageEntry, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    if path.is_dir() {
        for page in directory_page_paths(path)? {
            let name = page
                .strip_prefix(path)
                .unwrap_or(&page)
                .to_string_lossy()
                .to_string();
            let bytes = std::fs::read(&page).map_err(|e| format!("{}: {e}", page.display()))?;
            let entry = PageEntry {
                name,
                size: bytes.len() as u64,
                crc32: crc32fast::hash(&bytes),
            };
            f(&entry, &bytes)?;
        }
        return Ok(());
    }

    let mut zip = open_archive(path)?;
    let names = page_names_from(&mut zip);
    for name in names {
        let mut handle = zip
            .by_name(&name)
            .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
        let entry = PageEntry {
            name: name.clone(),
            size: handle.size(),
            crc32: handle.crc32(),
        };
        // Same clamp and same reasoning as `read_archive`: `size()` is
        // unverified archive metadata, so it drives a bounded hint and
        // `read_to_end` grows the buffer to the real size regardless.
        let hint = handle.size().min(MAX_ENTRY_CAPACITY_HINT) as usize;
        let mut bytes = Vec::with_capacity(hint);
        handle
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
        drop(handle);
        f(&entry, &bytes)?;
    }
    Ok(())
}
```

- [ ] **Step 5: Export both**

In `crates/taiyaku-read/src/lib.rs`, extend the `pages` re-export list at lines
17-18 to include `PageEntry`, `for_each_member_page` and `member_page_entries`,
keeping it alphabetical within its existing grouping.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p taiyaku-read`
Expected: PASS, all four new tests among them.

- [ ] **Step 7: Clippy**

Run: `cargo clippy -p taiyaku-read --all-targets -- -D warnings`
Expected: no output. If pedantic objects to `bytes.len() as u64`, use
`u64::try_from(bytes.len()).unwrap_or(u64::MAX)` with a one-line comment saying
a `usize` that does not fit a `u64` does not exist on any host this crate
targets — do **not** add `#[allow]`.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-read/Cargo.toml crates/taiyaku-read/src/pages.rs crates/taiyaku-read/src/lib.rs crates/taiyaku-read/src/tests.rs Cargo.lock
git commit -m "taiyaku-read describes and streams a member's pages, beside the two listings rather than instead of them"
```

---

### Task 2: `cbz.rs` — the layout, and a whole file emitted from it

**Files:**
- Create: `crates/taiyaku-serve/src/cbz.rs`
- Create: `crates/taiyaku-serve/src/cbz/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (add `mod cbz;` beside the other module declarations)

**Interfaces:**
- Consumes: nothing from earlier tasks. This module is pure: no I/O, no HTTP,
  no library rows. It is handed names, sizes and crcs and hands back bytes.
- Produces:
  - `pub(crate) struct PlannedEntry { pub name: String, pub size: u64, pub crc32: u32 }`
  - `pub(crate) struct Plan { … }` with:
    - `pub(crate) fn of(entries: Vec<PlannedEntry>) -> Plan`
    - `pub(crate) fn len(&self) -> u64` — the exact `Content-Length`
    - `pub(crate) fn entries(&self) -> &[PlannedEntry]`
    - `pub(crate) fn zip64(&self) -> bool`
    - `pub(crate) fn local_header(&self, index: usize) -> Vec<u8>`
    - `pub(crate) fn trailer(&self) -> Vec<u8>` — every central-directory record and the end record
    - `pub(crate) fn payload_span(&self, index: usize) -> (u64, u64)` — the absolute `[start, end)` of entry `index`'s bytes in the output

**The writer is hand-rolled rather than `zip::ZipWriter`, deliberately.** The
whole feature turns on knowing the output's length before the first byte and on
being able to resume from an arbitrary offset, and neither is something a
`Write + Seek` writer can tell you. What `zip` is used for here is the opposite
direction — Step 2's tests parse what this module produces, which is a stronger
check than asserting the arithmetic against itself.

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-serve/src/cbz/tests.rs`:

```rust
use super::{Plan, PlannedEntry};

/// Three pages and a metadata document, with the payloads a test can identify.
fn fixture() -> (Plan, Vec<(String, Vec<u8>)>) {
    let payloads: Vec<(String, Vec<u8>)> = vec![
        ("ComicInfo.xml".to_string(), b"<ComicInfo/>".to_vec()),
        ("0001.jpg".to_string(), b"page one bytes".to_vec()),
        ("0002.jpg".to_string(), b"page two".to_vec()),
        ("0003.png".to_string(), vec![0u8; 5000]),
    ];
    let entries = payloads
        .iter()
        .map(|(name, bytes)| PlannedEntry {
            name: name.clone(),
            size: bytes.len() as u64,
            crc32: crc32_of(bytes),
        })
        .collect();
    (Plan::of(entries), payloads)
}

/// The test's own crc, so a bug in the module's cannot hide behind it.
fn crc32_of(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Emit the whole file, the way `download.rs` will.
fn emit(plan: &Plan, payloads: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (index, (_, bytes)) in payloads.iter().enumerate() {
        out.extend_from_slice(&plan.local_header(index));
        out.extend_from_slice(bytes);
    }
    out.extend_from_slice(&plan.trailer());
    out
}

#[test]
fn the_promised_length_is_the_length_that_comes_out() {
    let (plan, payloads) = fixture();
    let bytes = emit(&plan, &payloads);
    assert_eq!(
        plan.len(),
        bytes.len() as u64,
        "Content-Length would have been a lie"
    );
}

#[test]
fn a_real_zip_reader_gets_the_names_order_sizes_and_crcs_back() {
    let (plan, payloads) = fixture();
    let bytes = emit(&plan, &payloads);

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
    assert_eq!(archive.len(), payloads.len());
    for (index, (name, expected)) in payloads.iter().enumerate() {
        let mut entry = archive.by_index(index).expect("entry by position");
        assert_eq!(entry.name(), name, "entry {index} is out of order");
        assert_eq!(entry.compression(), zip::CompressionMethod::Stored);
        assert_eq!(entry.size(), expected.len() as u64);
        assert_eq!(entry.crc32(), crc32_of(expected));
        let mut got = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut got).expect("reads back");
        assert_eq!(&got, expected);
    }
}

#[test]
fn a_payload_span_is_where_that_entrys_bytes_actually_land() {
    let (plan, payloads) = fixture();
    let bytes = emit(&plan, &payloads);
    for (index, (_, expected)) in payloads.iter().enumerate() {
        let (start, end) = plan.payload_span(index);
        assert_eq!(
            &bytes[start as usize..end as usize],
            expected.as_slice(),
            "entry {index}'s span does not point at its bytes"
        );
    }
}

#[test]
fn an_empty_edition_is_still_a_readable_zip() {
    // Not reachable from a real edition — `derive` does not create one and
    // `Edition::total_pages` would be 0 — but a plan of nothing must not
    // produce something a reader chokes on, because the arithmetic for the
    // trailer is the same arithmetic in both cases.
    let plan = Plan::of(Vec::new());
    let bytes = plan.trailer();
    assert_eq!(plan.len(), bytes.len() as u64);
    let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
    assert_eq!(archive.len(), 0);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p taiyaku-serve cbz::`
Expected: FAIL — `file not found for module 'cbz'` until Step 3 creates it, then
`Plan` unresolved.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-serve/src/cbz.rs`:

```rust
//! An edition as one `STORE` zip whose length is known before the first byte.
//!
//! **Why the writer is here rather than `zip::ZipWriter`.** Spec C2 §13.1's
//! whole argument for `STORE` is that the output's size is a deterministic
//! function of its entry names and sizes, so `Content-Length` is exact and
//! `Range` therefore works — and a `Write + Seek` writer cannot tell you either
//! of those before it has written the file. This module is the arithmetic that
//! can: `Plan::len` is the answer, `Plan::local_header` and `Plan::trailer` are
//! the bytes around the payloads, and `Plan::payload_span` is what lets a
//! resume find its way in.
//!
//! **`STORE` and not `DEFLATE`**, because the pages are already JPEGs. That is
//! §13.1's reasoning and it holds; what does *not* hold is the sentence beside
//! it calling generation "a copy". Measured on `corpus/` on 2026-08-23: 11,431
//! of 11,681 entries are `DEFLATE`, so producing this output inflates 97.9% of
//! them. `docs/notes.md` carries the figure. The consequence worth knowing is
//! that a generated CBZ is slightly **larger** than the archive it came from.
//!
//! No data descriptors: every crc is known before its local header, because an
//! archive member's comes free from its central directory and a directory
//! member's is hashed in `taiyaku_read::member_page_entries`. Spec §4 records
//! why that beat the streaming bit — this is the one artefact taiyaku produces
//! that leaves the server, and it fails on a device where nothing reports back.

/// A fixed DOS timestamp: 1980-01-01 00:00:00, the epoch of the field.
///
/// Constant rather than "now" so that two downloads of an unchanged edition are
/// byte-identical. A mutable clock here would also make every test in this
/// module's `tests.rs` depend on when it ran.
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0x0000;

/// Above this, a size or offset cannot be stated in a classic record.
const U32_MAX: u64 = 0xFFFF_FFFF;

const LOCAL_HEADER_FIXED: u64 = 30;
const CENTRAL_HEADER_FIXED: u64 = 46;
const END_RECORD: u64 = 22;
/// Header id + size + uncompressed + compressed, on a local header.
const ZIP64_LOCAL_EXTRA: u64 = 4 + 8 + 8;
/// The same plus the local header's offset, on a central-directory record.
const ZIP64_CENTRAL_EXTRA: u64 = 4 + 8 + 8 + 8;
const ZIP64_END_RECORD: u64 = 56;
const ZIP64_LOCATOR: u64 = 20;

/// One entry as the plan knows it: what it is called, how long it is, and the
/// crc32 of exactly those bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedEntry {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) crc32: u32,
}

/// A complete output layout, computed before anything is sent.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    entries: Vec<PlannedEntry>,
    /// Absolute offset of each entry's **local header**, parallel to `entries`.
    offsets: Vec<u64>,
    /// Where the first central-directory record starts.
    directory_at: u64,
    /// The total, which is what `Content-Length` states.
    total: u64,
    zip64: bool,
}

impl Plan {
    /// Lay out `entries` in the order given.
    ///
    /// The order **is** the reading order; this module does not sort. Deciding
    /// which page comes next is `docs/sidecar.md`'s normative rule and
    /// `download.rs` applies it, because a second sort here would be a second
    /// derivation of it.
    pub(crate) fn of(entries: Vec<PlannedEntry>) -> Self {
        // Two passes, because whether a classic record can hold this file
        // decides the size of every record in it. A single pass would have to
        // guess and re-do.
        let classic = Self::lay_out(&entries, false);
        let needs_zip64 = classic.2 > U32_MAX
            || classic.1 > U32_MAX
            || entries.iter().any(|e| e.size > U32_MAX)
            || entries.len() > usize::from(u16::MAX);
        let (offsets, directory_at, total) = if needs_zip64 {
            Self::lay_out(&entries, true)
        } else {
            classic
        };
        Self {
            entries,
            offsets,
            directory_at,
            total,
            zip64: needs_zip64,
        }
    }

    /// `(local header offsets, where the directory starts, total length)`.
    fn lay_out(entries: &[PlannedEntry], zip64: bool) -> (Vec<u64>, u64, u64) {
        let local_extra = if zip64 { ZIP64_LOCAL_EXTRA } else { 0 };
        let central_extra = if zip64 { ZIP64_CENTRAL_EXTRA } else { 0 };
        let mut offsets = Vec::with_capacity(entries.len());
        let mut at = 0u64;
        for entry in entries {
            offsets.push(at);
            at += LOCAL_HEADER_FIXED + entry.name.len() as u64 + local_extra + entry.size;
        }
        let directory_at = at;
        let directory: u64 = entries
            .iter()
            .map(|e| CENTRAL_HEADER_FIXED + e.name.len() as u64 + central_extra)
            .sum();
        let tail = if zip64 {
            ZIP64_END_RECORD + ZIP64_LOCATOR + END_RECORD
        } else {
            END_RECORD
        };
        (offsets, directory_at, directory_at + directory + tail)
    }

    pub(crate) fn len(&self) -> u64 {
        self.total
    }

    pub(crate) fn entries(&self) -> &[PlannedEntry] {
        &self.entries
    }

    pub(crate) fn zip64(&self) -> bool {
        self.zip64
    }

    /// The absolute `[start, end)` of entry `index`'s payload in the output.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not an entry of this plan, which is a caller bug:
    /// every caller iterates `entries()`.
    pub(crate) fn payload_span(&self, index: usize) -> (u64, u64) {
        let entry = &self.entries[index];
        let local_extra = if self.zip64 { ZIP64_LOCAL_EXTRA } else { 0 };
        let start =
            self.offsets[index] + LOCAL_HEADER_FIXED + entry.name.len() as u64 + local_extra;
        (start, start + entry.size)
    }

    /// Entry `index`'s local file header.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not an entry of this plan — see `payload_span`.
    pub(crate) fn local_header(&self, index: usize) -> Vec<u8> {
        let entry = &self.entries[index];
        let mut out = Vec::with_capacity(64 + entry.name.len());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&if self.zip64 { 45u16 } else { 20u16 }.to_le_bytes());
        // No flags. The names this module is given are ASCII by construction —
        // `download.rs` numbers them — so the UTF-8 bit would state something
        // true and buy nothing, and a zero here is the most conservative
        // header a reader can be handed.
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // STORE
        out.extend_from_slice(&DOS_TIME.to_le_bytes());
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&entry.crc32.to_le_bytes());
        let stated = if self.zip64 { U32_MAX } else { entry.size };
        // Compressed and uncompressed are the same number: this is STORE.
        out.extend_from_slice(&(stated as u32).to_le_bytes());
        out.extend_from_slice(&(stated as u32).to_le_bytes());
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(
            &(if self.zip64 { ZIP64_LOCAL_EXTRA as u16 } else { 0 }).to_le_bytes(),
        );
        out.extend_from_slice(entry.name.as_bytes());
        if self.zip64 {
            out.extend_from_slice(&0x0001u16.to_le_bytes());
            out.extend_from_slice(&16u16.to_le_bytes());
            out.extend_from_slice(&entry.size.to_le_bytes());
            out.extend_from_slice(&entry.size.to_le_bytes());
        }
        out
    }

    /// Every central-directory record, then the end record(s).
    pub(crate) fn trailer(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.total - self.directory_at) as usize);
        for (index, entry) in self.entries.iter().enumerate() {
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            // Made by: 0x031E — Unix, zip 3.0. Needed: as the local header.
            out.extend_from_slice(&0x031Eu16.to_le_bytes());
            out.extend_from_slice(&if self.zip64 { 45u16 } else { 20u16 }.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // STORE
            out.extend_from_slice(&DOS_TIME.to_le_bytes());
            out.extend_from_slice(&DOS_DATE.to_le_bytes());
            out.extend_from_slice(&entry.crc32.to_le_bytes());
            let stated = if self.zip64 { U32_MAX } else { entry.size };
            out.extend_from_slice(&(stated as u32).to_le_bytes());
            out.extend_from_slice(&(stated as u32).to_le_bytes());
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            out.extend_from_slice(
                &(if self.zip64 { ZIP64_CENTRAL_EXTRA as u16 } else { 0 }).to_le_bytes(),
            );
            out.extend_from_slice(&0u16.to_le_bytes()); // comment length
            out.extend_from_slice(&0u16.to_le_bytes()); // disk number
            out.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            // 0o644 in the high half, the DOS archive bit clear.
            out.extend_from_slice(&0x81A4_0000u32.to_le_bytes());
            let offset = if self.zip64 { U32_MAX } else { self.offsets[index] };
            out.extend_from_slice(&(offset as u32).to_le_bytes());
            out.extend_from_slice(entry.name.as_bytes());
            if self.zip64 {
                out.extend_from_slice(&0x0001u16.to_le_bytes());
                out.extend_from_slice(&24u16.to_le_bytes());
                out.extend_from_slice(&entry.size.to_le_bytes());
                out.extend_from_slice(&entry.size.to_le_bytes());
                out.extend_from_slice(&self.offsets[index].to_le_bytes());
            }
        }

        let directory_size = self.total
            - self.directory_at
            - if self.zip64 {
                ZIP64_END_RECORD + ZIP64_LOCATOR + END_RECORD
            } else {
                END_RECORD
            };
        let count = self.entries.len() as u64;

        if self.zip64 {
            let zip64_end_at = self.directory_at + directory_size;
            out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
            out.extend_from_slice(&(ZIP64_END_RECORD - 12).to_le_bytes()); // size of the rest
            out.extend_from_slice(&0x031Eu16.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // this disk
            out.extend_from_slice(&0u32.to_le_bytes()); // disk with the directory
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&directory_size.to_le_bytes());
            out.extend_from_slice(&self.directory_at.to_le_bytes());

            out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // disk with the zip64 end record
            out.extend_from_slice(&zip64_end_at.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes()); // total disks
        }

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        out.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
        let stated_count = if self.zip64 { u16::MAX } else { count as u16 };
        out.extend_from_slice(&stated_count.to_le_bytes());
        out.extend_from_slice(&stated_count.to_le_bytes());
        let stated_size = if self.zip64 { U32_MAX } else { directory_size };
        out.extend_from_slice(&(stated_size as u32).to_le_bytes());
        let stated_at = if self.zip64 { U32_MAX } else { self.directory_at };
        out.extend_from_slice(&(stated_at as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        out
    }
}

#[cfg(test)]
mod tests;
```

Add `mod cbz;` to `crates/taiyaku-serve/src/lib.rs` beside the other module
declarations, in alphabetical position.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p taiyaku-serve cbz::`
Expected: PASS, four tests.

If `the_promised_length_is_the_length_that_comes_out` fails, the mismatch is in
one of the six constants at the top of the module — print
`plan.len()` against `bytes.len()` and the difference will name which. Do not
"fix" it by adjusting `total`; fix the constant that is wrong.

- [ ] **Step 5: Clippy**

Run: `cargo clippy -p taiyaku-serve --all-targets -- -D warnings`
Expected: no output. Note what this does and does not catch: it denies the
**default** lints, and `clippy::pedantic` is a separate advisory pass in
`check-all.sh` that cannot fail the build. So the casts in this module
(`as u32` on a value already bounded by `U32_MAX`, `as u16` on a length already
checked against `u16::MAX`) will not stop you — but each has its bound
established three lines above it, so say so in a comment rather than leaving a
reader to re-derive it, and never reach for `#[allow]`.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/cbz.rs crates/taiyaku-serve/src/cbz/tests.rs crates/taiyaku-serve/src/lib.rs
git commit -m "The CBZ layout: an exact length before the first byte, checked by parsing what it produces"
```

---

### Task 3: zip64, tested as arithmetic

**Files:**
- Modify: `crates/taiyaku-serve/src/cbz/tests.rs`

**Interfaces:**
- Consumes: `Plan::of`, `Plan::len`, `Plan::zip64`, `Plan::payload_span` from Task 2.
- Produces: nothing new. This task is the test that Task 2's zip64 branch is
  reachable and self-consistent.

**Nothing streams 4 GiB.** Spec §10 states the limit of this task plainly: the
layout function is checked against entry sizes above 32 bits, and no test
produces the bytes, because material that large would be a test deleted the
first time it made the suite slow. The corpus measurement — largest archive
1.49 GB, largest group 1.49 GB, 0 of 47 over 4 GiB — is the evidence for how
unlikely the branch is, not for its correctness.

- [ ] **Step 0: Expect the build to tell you to delete something**

Task 2 put `#[expect(dead_code, reason = "…")]` on `Plan::entries` and
`Plan::zip64`, because no test read them yet. **This task's tests read
`zip64()` and `payload_span()` — and NOT `entries()`.** *(An earlier draft of
this step said "your tests read both", which is false; the implementer removed
both as instructed, watched clippy fail with `method 'entries' is never used`,
and restored the one still needed. Corrected here.)*

So: delete the `#[expect]` on `zip64()`, because `#[expect]` fails the build the
moment the lint stops firing — that is why it was chosen over `#[allow]`. Leave
the one on `entries()`, narrowing its `reason` to name Task 7 alone. Leave the
module-level `#![cfg_attr(not(test), expect(dead_code, …))]` until Task 7 wires
the route. **Do not invent a test that calls `entries()` merely to make it
used** — a test written to satisfy an attribute asserts nothing.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/cbz/tests.rs`:

```rust
/// A plan whose entries are declared enormous, so the zip64 branch is taken.
/// No bytes exist for them and none are produced.
fn oversized() -> Plan {
    Plan::of(vec![
        PlannedEntry { name: "0001.jpg".into(), size: 3_000_000_000, crc32: 1 },
        PlannedEntry { name: "0002.jpg".into(), size: 3_000_000_000, crc32: 2 },
    ])
}

#[test]
fn a_total_past_four_gibibytes_takes_the_zip64_branch() {
    let plan = oversized();
    assert!(plan.zip64(), "6 GB of payload cannot be stated in a classic record");
    // Every record grows: the two local extras, the two central extras, the
    // zip64 end record and its locator.
    let classic_would_be = 2 * (30 + 8) + 6_000_000_000 + 2 * (46 + 8) + 22;
    assert_eq!(plan.len(), classic_would_be + 2 * 20 + 2 * 28 + 56 + 20);
}

#[test]
fn one_entry_past_four_gibibytes_takes_it_too() {
    // The total here is under the classic ceiling only because there is one
    // entry; the *entry* is not, and a classic local header has nowhere to
    // say so.
    let plan = Plan::of(vec![PlannedEntry {
        name: "0001.jpg".into(),
        size: 5_000_000_000,
        crc32: 7,
    }]);
    assert!(plan.zip64());
}

#[test]
fn a_zip64_payload_span_accounts_for_the_extra_field() {
    let plan = oversized();
    let (start, end) = plan.payload_span(0);
    // 30 fixed + 8 name + 20 zip64 extra.
    assert_eq!(start, 58);
    assert_eq!(end - start, 3_000_000_000);
    let (second, _) = plan.payload_span(1);
    assert_eq!(second, 58 + 3_000_000_000 + 58);
}

#[test]
fn an_ordinary_edition_does_not_take_it() {
    let (plan, _) = fixture();
    assert!(
        !plan.zip64(),
        "nothing in corpus/ reaches this branch — largest group 1.49 GB, 0 of 47 over 4 GiB"
    );
}
```

- [ ] **Step 2: Run and watch fail**

Run: `cargo test -p taiyaku-serve cbz::tests`
Expected: FAIL on the arithmetic assertions if any of Task 2's zip64 constants
is wrong; PASS on `an_ordinary_edition_does_not_take_it` immediately.

- [ ] **Step 3: Fix whichever constant is wrong**

Only Task 2's six constants and the two `lay_out` branches can be at fault.
Do not change the expected values in the test to match the code: the test's
numbers are derived from the format, and the format is not negotiable.

- [ ] **Step 4: Run**

Run: `cargo test -p taiyaku-serve cbz::`
Expected: PASS, eight tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/cbz/tests.rs crates/taiyaku-serve/src/cbz.rs
git commit -m "zip64 as arithmetic: reachable, self-consistent, and unexercised by any real material"
```

---

### Task 4: emitting from an arbitrary offset

**Files:**
- Modify: `crates/taiyaku-serve/src/cbz.rs`
- Modify: `crates/taiyaku-serve/src/cbz/tests.rs`

**Interfaces:**
- Consumes: everything from Task 2.
- Produces: `pub(crate) fn slice_of(&self, region: &[u8], region_start: u64, from: u64, upto: u64) -> &[u8]`
  — no. **Produces:** `pub(crate) struct Cursor` is not needed either. The one
  new thing is a free function:
  - `pub(crate) fn clipped(bytes: &[u8], absolute_start: u64, from: u64, upto: u64) -> &[u8]`
    — given a run of bytes that lands at `absolute_start` in the output, the
    part of it inside `[from, upto)`. Empty when the run is entirely outside.

That single helper is all a resume needs: `download.rs` walks headers, payloads
and the trailer in order, keeps a running absolute offset, and passes each run
through `clipped`. Spec §5's "produce from zero and discard the prefix" is
exactly this, and it is the whole of it.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-serve/src/cbz/tests.rs`:

```rust
use super::clipped;

#[test]
fn clipped_keeps_only_the_part_inside_the_window() {
    let run = b"abcdefghij";
    // Wholly inside.
    assert_eq!(clipped(run, 100, 0, 1000), run);
    // Trimmed at the front.
    assert_eq!(clipped(run, 100, 103, 1000), b"defghij");
    // Trimmed at the back.
    assert_eq!(clipped(run, 100, 0, 104), b"abcd");
    // Both ends.
    assert_eq!(clipped(run, 100, 102, 105), b"cde");
    // Entirely before the window.
    assert_eq!(clipped(run, 100, 200, 300), b"");
    // Entirely after it.
    assert_eq!(clipped(run, 100, 0, 100), b"");
    // A window that is empty is empty everywhere.
    assert_eq!(clipped(run, 100, 105, 105), b"");
}

/// Emit `[from, upto)` of the output, the way `download.rs` will.
fn emit_range(plan: &Plan, payloads: &[(String, Vec<u8>)], from: u64, upto: u64) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut at = 0u64;
    for (index, (_, bytes)) in payloads.iter().enumerate() {
        let header = plan.local_header(index);
        out.extend_from_slice(clipped(&header, at, from, upto));
        at += header.len() as u64;
        out.extend_from_slice(clipped(bytes, at, from, upto));
        at += bytes.len() as u64;
    }
    let trailer = plan.trailer();
    out.extend_from_slice(clipped(&trailer, at, from, upto));
    out
}

#[test]
fn every_split_of_the_output_reassembles_into_the_whole() {
    let (plan, payloads) = fixture();
    let whole = emit(&plan, &payloads);
    let total = plan.len();

    // Deterministic "random": a small LCG, so a failure is reproducible and no
    // dependency is added for it.
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let a = seed % (total + 1);
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let b = seed % (total + 1);
        let (from, upto) = if a <= b { (a, b) } else { (b, a) };

        let part = emit_range(&plan, &payloads, from, upto);
        assert_eq!(
            part.as_slice(),
            &whole[from as usize..upto as usize],
            "the range [{from}, {upto}) is not the same bytes as the whole file's"
        );
    }

    // And the degenerate ones the generator will not reliably produce.
    assert_eq!(emit_range(&plan, &payloads, 0, total), whole);
    assert_eq!(emit_range(&plan, &payloads, total, total), Vec::<u8>::new());
    assert_eq!(emit_range(&plan, &payloads, total - 1, total), vec![whole[whole.len() - 1]]);
}
```

- [ ] **Step 2: Run and watch fail**

Run: `cargo test -p taiyaku-serve cbz::tests::clipped`
Expected: FAIL — `cannot find function 'clipped'`.

- [ ] **Step 3: Implement it**

Add to `crates/taiyaku-serve/src/cbz.rs`, after the `impl Plan` block:

```rust
/// The part of one run of output bytes that falls inside `[from, upto)`.
///
/// **This is the whole of spec §5's resume.** A generated download does not
/// seek: it walks the same headers, payloads and trailer it would have sent
/// from the start, keeps a running absolute offset, and passes every run
/// through here. Runs before the window come back empty and cost nothing but
/// the read that produced them; the run the window starts in is trimmed at the
/// front; everything after `upto` is empty again, so a bounded range stops
/// producing rather than running to the end.
///
/// What that leaves on the table, deliberately: the *read* of a payload before
/// the window still happens, so resuming at 300 MB inflates the first 300 MB
/// and discards it. The better version resolves an offset straight to
/// `(entry, offset within entry)` — `Plan::payload_span` is already the map it
/// needs — and is deferred behind a measurement, because the contract does not
/// change when it lands. Spec §5 and §12.
///
/// `region_start` is where `bytes` begins in the output. `upto` is exclusive.
pub(crate) fn clipped(bytes: &[u8], region_start: u64, from: u64, upto: u64) -> &[u8] {
    let region_end = region_start + bytes.len() as u64;
    if region_end <= from || region_start >= upto || from >= upto {
        return &[];
    }
    let start = from.saturating_sub(region_start) as usize;
    let end = (upto.min(region_end) - region_start) as usize;
    &bytes[start..end]
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p taiyaku-serve cbz::`
Expected: PASS. **Do not treat the count as the assertion** — Task 3's fix wave
added a test after this number was written, so it is eleven, not the ten an
earlier draft said. What matters is that nothing failed.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/cbz.rs crates/taiyaku-serve/src/cbz/tests.rs
git commit -m "A range of the generated CBZ is the same bytes as that slice of the whole, checked over 64 random splits"
```

---

### Task 5: `comicinfo.rs` — the metadata document from library rows

**Files:**
- Create: `crates/taiyaku-serve/src/comicinfo.rs`
- Create: `crates/taiyaku-serve/src/comicinfo/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod comicinfo;`)

**Interfaces:**
- Consumes: `rusqlite::Connection`, already a direct dependency of this crate.
- Produces:
  - `pub(crate) struct ComicInfo`, every field `pub(crate)`: `series: Option<String>`, `number: Option<f64>`, `title: Option<String>`, `page_count: u32`, `language: Option<String>`, `credits: Vec<(String, String)>`, `notes: Option<String>` — `credits` is `(role, person name)`
  - `pub(crate) fn read(conn: &Connection, slug: &str, edition_key: &str, page_count: u32) -> Result<ComicInfo, String>`
  - `pub(crate) fn xml(info: &ComicInfo) -> String`

**Roles map to ComicInfo elements**, and the mapping is the only judgement in
this module: `credit.role` is one of `author | artist | translator | group |
publisher` (`schema/001_initial.sql:149`). `author` → `Writer`, `artist` →
`Penciller`, `translator` → `Translator`, `publisher` → `Publisher`, and
**`group` → `Publisher` as well** — a scanlation group is who published that
edition, which is the fact `credit.edition_id` exists to hold and the one Komga
has nowhere to put. Several people in one role become one comma-separated
element, which is what the format specifies.

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-serve/src/comicinfo/tests.rs`:

```rust
use super::{ComicInfo, xml};

fn full() -> ComicInfo {
    ComicInfo {
        series: Some("Go-Toubun no Hanayome".to_string()),
        number: Some(1.0),
        title: Some("Go-Toubun no Hanayome, volume 1".to_string()),
        page_count: 194,
        language: Some("ja".to_string()),
        credits: vec![
            ("author".to_string(), "Negi Haruba".to_string()),
            ("translator".to_string(), "A Translator".to_string()),
            ("group".to_string(), "Some Group".to_string()),
            ("publisher".to_string(), "Kodansha".to_string()),
        ],
        notes: Some("Japanese raw".to_string()),
    }
}

#[test]
fn every_fact_that_exists_becomes_an_element() {
    let out = xml(&full());
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n"));
    assert!(out.contains("<Series>Go-Toubun no Hanayome</Series>"));
    assert!(out.contains("<Number>1</Number>"), "an integral volume drops the .0");
    assert!(out.contains("<PageCount>194</PageCount>"));
    assert!(out.contains("<LanguageISO>ja</LanguageISO>"));
    assert!(out.contains("<Writer>Negi Haruba</Writer>"));
    assert!(out.contains("<Translator>A Translator</Translator>"));
    assert!(out.contains("<Notes>Japanese raw</Notes>"));
}

#[test]
fn a_group_and_a_publisher_share_one_element() {
    let out = xml(&full());
    assert!(
        out.contains("<Publisher>Some Group, Kodansha</Publisher>"),
        "a scanlation group is who published that edition; got {out}"
    );
}

#[test]
fn a_half_volume_keeps_its_fraction() {
    let mut info = full();
    info.number = Some(1.5);
    assert!(xml(&info).contains("<Number>1.5</Number>"));
}

#[test]
fn an_absent_fact_is_an_absent_element_not_an_empty_one() {
    let info = ComicInfo {
        series: None,
        number: None,
        title: None,
        page_count: 3,
        language: None,
        credits: Vec::new(),
        notes: None,
    };
    let out = xml(&info);
    for absent in ["<Series", "<Number", "<Title", "<LanguageISO", "<Writer", "<Publisher", "<Notes"] {
        assert!(
            !out.contains(absent),
            "an empty <Writer/> claims the writer is unknown; leaving it out claims nothing. Found {absent} in {out}"
        );
    }
    // The one element that is always true.
    assert!(out.contains("<PageCount>3</PageCount>"));
}

#[test]
fn a_title_with_markup_in_it_is_escaped() {
    let mut info = full();
    info.title = Some("A & B <not a tag>".to_string());
    let out = xml(&info);
    assert!(out.contains("<Title>A &amp; B &lt;not a tag&gt;</Title>"));
}
```

- [ ] **Step 2: Run and watch fail**

Run: `cargo test -p taiyaku-serve comicinfo::`
Expected: FAIL — module not found.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-serve/src/comicinfo.rs`:

```rust
//! The `ComicInfo.xml` a generated CBZ carries, built from the library's rows.
//!
//! **Only the generated path.** A passthrough download is served byte for byte
//! and gains nothing: whatever `ComicInfo.xml` the source archive holds travels
//! unchanged, and 12 of this corpus's 57 archives hold one (`docs/sidecar.md`).
//! Spec C2 §3 and §6.
//!
//! **An absent fact is an absent element.** A `<Writer/>` with nothing in it
//! claims the writer is unknown; leaving it out claims nothing. Most credit
//! fields *are* absent today — `credit` has no writer until spec C3's metadata
//! editing ships, which is O44 — so in practice a download carries series,
//! number, title, page count and the edition label. Said here rather than
//! discovered later.
//!
//! **`LanguageISO` comes from `edition.language`**, a real column the rule
//! engine's `set_language` writes and `import` preserves — not from the edition
//! key. Deriving a language from `raw` or `en` would be a guess presented as
//! metadata, which is what `work.root_id` was rejected for in spec C1.

use std::fmt::Write as _;

use rusqlite::Connection;

/// The facts a `ComicInfo.xml` states, each optional because each may be
/// genuinely unknown.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComicInfo {
    pub(crate) series: Option<String>,
    pub(crate) number: Option<f64>,
    pub(crate) title: Option<String>,
    pub(crate) page_count: u32,
    pub(crate) language: Option<String>,
    /// `(role, person name)` as `credit` holds them, in a stable order.
    pub(crate) credits: Vec<(String, String)>,
    pub(crate) notes: Option<String>,
}

/// What this library knows about one edition of one work.
///
/// # Errors
///
/// Returns an error if the rows cannot be read, or if no work has this slug —
/// which a caller reaches only after the visibility check has already passed,
/// so it means the work went away between the two.
pub(crate) fn read(
    conn: &Connection,
    slug: &str,
    edition_key: &str,
    page_count: u32,
) -> Result<ComicInfo, String> {
    let (work_id, title, number, series_id): (i64, String, Option<f64>, Option<i64>) = conn
        .query_row(
            "SELECT id, title, volume, series_id FROM work WHERE slug = ?1",
            [slug],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| format!("reading work `{slug}` for its metadata: {e}"))?;

    let series = match series_id {
        None => None,
        Some(id) => conn
            .query_row(
                // `is_primary` first, then any title, then the sort title —
                // a series row always has the last of the three.
                "SELECT COALESCE(
                     (SELECT title FROM series_title
                       WHERE series_id = ?1 ORDER BY is_primary DESC, title LIMIT 1),
                     sort_title)
                   FROM series WHERE id = ?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| format!("reading the series of `{slug}`: {e}"))
            .map(Some)?,
    };

    let (edition_id, label, language): (i64, String, Option<String>) = conn
        .query_row(
            "SELECT id, label, language FROM edition WHERE work_id = ?1 AND key = ?2",
            rusqlite::params![work_id, edition_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| format!("reading edition `{edition_key}` of `{slug}`: {e}"))?;

    // Series-level, work-level and edition-level credits all apply to this
    // download, which is what the three nullable columns are for. Ordered so
    // the output is stable between two downloads of the same edition.
    let mut stmt = conn
        .prepare(
            "SELECT c.role, p.name FROM credit c JOIN person p ON p.id = c.person_id
              WHERE c.edition_id = ?1 OR c.work_id = ?2
                 OR (c.series_id IS NOT NULL AND c.series_id = ?3)
              ORDER BY c.role, p.name",
        )
        .map_err(|e| format!("preparing the credits of `{slug}`: {e}"))?;
    let credits: Vec<(String, String)> = stmt
        .query_map(
            rusqlite::params![edition_id, work_id, series_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("reading the credits of `{slug}`: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading the credits of `{slug}`: {e}"))?;

    Ok(ComicInfo {
        series,
        number,
        title: Some(title),
        page_count,
        language,
        credits,
        notes: Some(label),
    })
}

/// Which element each `credit.role` becomes.
///
/// **`group` is a `Publisher`**: a scanlation group is who published that
/// edition, which is the fact `credit.edition_id` exists to hold and the one
/// Komga has nowhere to put. A role this does not know is dropped rather than
/// invented into an element name no reader would recognise.
fn element_for(role: &str) -> Option<&'static str> {
    match role {
        "author" => Some("Writer"),
        "artist" => Some("Penciller"),
        "translator" => Some("Translator"),
        "group" | "publisher" => Some("Publisher"),
        _ => None,
    }
}

/// The five characters XML cannot carry raw, and nothing else.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// A volume number as a person writes it: `1`, not `1.0`; `1.5` stays `1.5`.
///
/// `work.volume` is REAL because omnibus and half-volumes exist
/// (`schema/001_initial.sql`), so the fraction has to survive and the absent
/// fraction has to go.
fn number_text(volume: f64) -> String {
    if volume.fract() == 0.0 && volume.abs() < 1e15 {
        format!("{:.0}", volume)
    } else {
        let mut text = format!("{volume}");
        if text.ends_with(".0") {
            text.truncate(text.len() - 2);
        }
        text
    }
}

/// The document, as the bytes that go into the archive.
pub(crate) fn xml(info: &ComicInfo) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo>\n");
    let mut element = |name: &str, value: &str| {
        let _ = writeln!(out, "  <{name}>{}</{name}>", escape(value));
    };
    if let Some(series) = &info.series {
        element("Series", series);
    }
    if let Some(number) = info.number {
        element("Number", &number_text(number));
    }
    if let Some(title) = &info.title {
        element("Title", title);
    }
    element("PageCount", &info.page_count.to_string());
    if let Some(language) = &info.language {
        element("LanguageISO", language);
    }
    // One element per ComicInfo field, several people comma-separated inside
    // it, which is what the format specifies. `element_for`'s order fixes the
    // element order, so two downloads of the same edition agree.
    for name in ["Writer", "Penciller", "Translator", "Publisher"] {
        let people: Vec<&str> = info
            .credits
            .iter()
            .filter(|(role, _)| element_for(role) == Some(name))
            .map(|(_, person)| person.as_str())
            .collect();
        if !people.is_empty() {
            element(name, &people.join(", "));
        }
    }
    if let Some(notes) = &info.notes {
        element("Notes", notes);
    }
    out.push_str("</ComicInfo>\n");
    out
}

#[cfg(test)]
mod tests;
```

Add `mod comicinfo;` to `crates/taiyaku-serve/src/lib.rs`.

- [ ] **Step 4: Run**

Run: `cargo test -p taiyaku-serve comicinfo::`
Expected: PASS, five tests.

- [ ] **Step 5: Add a test over real rows**

Append to `crates/taiyaku-serve/src/comicinfo/tests.rs`. It uses the fixture
library helper so the SQL in `read` is exercised rather than only the XML:

```rust
#[test]
fn read_gets_the_series_the_language_and_the_label_out_of_the_rows() {
    let library = crate::test_support::library();
    let conn = library.conn();
    conn.execute("INSERT INTO series (id, sort_title) VALUES (1, 'Sortable')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         VALUES (1, 'Alternative', 'alt', 0), (1, 'The Primary One', 'english', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO work (id, content_key, slug, series_id, volume, title, reading, hub)
         VALUES (1, 'k', 'w-v01', 1, 1.0, 'Volume One', 'rtl', 'raw')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO edition (id, work_id, key, label, language)
         VALUES (1, 1, 'raw', 'Japanese raw', 'ja')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO person (id, name) VALUES (1, 'Negi Haruba')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO credit (person_id, role, series_id) VALUES (1, 'author', 1)",
        [],
    )
    .unwrap();

    let info = super::read(&conn, "w-v01", "raw", 194).unwrap();

    assert_eq!(info.series.as_deref(), Some("The Primary One"), "is_primary wins");
    assert_eq!(info.language.as_deref(), Some("ja"));
    assert_eq!(info.notes.as_deref(), Some("Japanese raw"));
    assert_eq!(info.number, Some(1.0));
    assert_eq!(info.page_count, 194);
    assert_eq!(info.credits, vec![("author".to_string(), "Negi Haruba".to_string())]);
}
```

Run: `cargo test -p taiyaku-serve comicinfo::`
Expected: PASS, six tests.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/comicinfo.rs crates/taiyaku-serve/src/comicinfo/tests.rs crates/taiyaku-serve/src/lib.rs
git commit -m "ComicInfo.xml from the library's own rows, with an absent fact left absent rather than emptied"
```

---

### Task 6: the download filename

**Files:**
- Create: `crates/taiyaku-serve/src/download.rs` (the filename half only — the handlers arrive in Task 7)
- Create: `crates/taiyaku-serve/src/download/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`pub(crate) mod download;`)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub(crate) fn download_name(series: Option<&str>, volume: Option<f64>, title: &str, label: &str, extension: &str) -> String`
  - `pub(crate) fn content_disposition(name: &str) -> String`

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-serve/src/download/tests.rs`:

```rust
use super::{content_disposition, download_name};

#[test]
fn a_full_name_reads_the_way_a_downloads_folder_wants() {
    assert_eq!(
        download_name(Some("Go-Toubun no Hanayome"), Some(1.0), "ignored", "raw", "cbz"),
        "Go-Toubun no Hanayome - v01 (raw).cbz"
    );
}

#[test]
fn a_half_volume_keeps_its_fraction_and_a_big_one_its_digits() {
    assert_eq!(
        download_name(Some("S"), Some(1.5), "t", "raw", "cbz"),
        "S - v01.5 (raw).cbz"
    );
    assert_eq!(
        download_name(Some("S"), Some(12.0), "t", "raw", "cbz"),
        "S - v12 (raw).cbz"
    );
    assert_eq!(
        download_name(Some("S"), Some(105.0), "t", "raw", "cbz"),
        "S - v105 (raw).cbz"
    );
}

#[test]
fn no_series_falls_back_to_the_work_title() {
    // A hand-imported work has no series: `import` never writes `series_id`
    // and cannot, since `taiyaku.json` has no field for it.
    assert_eq!(
        download_name(None, Some(1.0), "Some Hand Import", "raw", "cbz"),
        "Some Hand Import - v01 (raw).cbz"
    );
}

#[test]
fn no_volume_drops_the_segment_rather_than_writing_v0() {
    assert_eq!(
        download_name(Some("S"), None, "t", "raw", "cbz"),
        "S (raw).cbz"
    );
}

#[test]
fn separators_and_controls_cannot_reach_the_filename() {
    // A title that arrived from a filename can hold anything a filename can.
    let name = download_name(Some("a/b\\c"), Some(1.0), "t", "d\u{7}e", "cbz");
    assert!(!name.contains('/'), "got {name}");
    assert!(!name.contains('\\'), "got {name}");
    assert!(!name.contains('\u{7}'), "got {name}");
    assert_eq!(name, "a-b-c - v01 (d-e).cbz");
}

#[test]
fn a_leading_dot_cannot_make_it_a_hidden_file() {
    assert_eq!(
        download_name(Some(".hidden"), None, "t", "raw", "cbz"),
        "hidden (raw).cbz"
    );
}

#[test]
fn the_extension_follows_the_bytes_not_the_route() {
    // A single-member edition may not be a comic archive at all: `member.format`
    // admits `pdf` and the two epub kinds. Spec §7.
    assert_eq!(
        download_name(Some("S"), Some(1.0), "t", "raw", "pdf"),
        "S - v01 (raw).pdf"
    );
}

#[test]
fn a_non_ascii_name_travels_in_both_forms() {
    let header = content_disposition("五等分の花嫁 - v01 (raw).cbz");
    // The ASCII form a client that understands only RFC 2183 will read.
    assert!(header.starts_with("attachment; filename=\""), "got {header}");
    assert!(
        header.contains("filename*=UTF-8''"),
        "a client that understands RFC 6266 must get the real name: {header}"
    );
    // Percent-encoded, and the parts that are already safe left alone.
    assert!(header.contains("-%20v01%20%28raw%29.cbz"), "got {header}");
    // The quoted fallback must not contain a raw quote or backslash.
    let fallback = header
        .split_once("filename=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name.to_string())
        .expect("a quoted fallback");
    assert!(!fallback.contains('"'));
    assert!(!fallback.contains('\\'));
    assert!(fallback.is_ascii(), "the fallback must be ASCII: {fallback}");
}

#[test]
fn a_quote_in_a_title_cannot_break_out_of_the_header() {
    let header = content_disposition("a\"b.cbz");
    assert!(!header.contains("\"a\"b"), "got {header}");
}
```

- [ ] **Step 2: Run and watch fail**

Run: `cargo test -p taiyaku-serve download::`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

> **The code below shipped with three defects, all fixed in `50ee9ac`. If you
> are re-running this plan, take the committed file rather than this block.**
> `volume_text` printed a raw `f64` remainder, so `2.3` became
> `"02.2999999999999998"` — `1.5` is exact in binary, which is why the test
> beside it never noticed. `safe` could return `""`, giving a filename that
> began with a space. And `safe`'s repeated-strip was single-pass, so
> `". .hidden"` kept its dot while the doc comment promised a leading dot
> "cannot make the download a hidden file". Each was found by running the
> function against inputs its tests never tried, not by the tests.

Create `crates/taiyaku-serve/src/download.rs`. **Note the module-level
attribute** — nothing calls this module until Task 7 registers the routes, so
`-D warnings` denies its `pub(crate)` items as dead. `cbz.rs` and
`comicinfo.rs` both hit this and both answered it the same way, following the
precedent spec A2 set in `auth.rs` for `Principal.roots`: `#[expect]`, never
`#[allow]`, so the day the route calls in, the expectation goes unfulfilled and
the build fails until the attribute comes off. It cannot linger past the task
that makes it true. *(Tasks 2 and 5 each discovered this the hard way because
their plan text omitted it; this one says so up front.)*

```rust
//! Downloading a member's own bytes, and an edition as one streamed CBZ.
//!
//! Spec C2, `docs/superpowers/specs/2026-08-23-library-download-design.md`.
//! `cbz.rs` is the layout and `comicinfo.rs` is the metadata document; this
//! module is the HTTP part — which shape of body, what headers, and which of
//! the two guards refused.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the filename half of the download module; Task 7 registers \
                   the routes that call it"
    )
)]

/// What a downloaded file is called.
///
/// `Series - v01 (raw).cbz`. Series, because that is what groups a downloads
/// folder; the work title when there is no series, which is every
/// hand-imported work, since `import` never writes `series_id` and cannot —
/// `taiyaku.json` has no field for it. No volume drops the segment rather than
/// claiming volume 0.
///
/// **`extension` is the caller's, not this function's**, because a
/// single-member edition served byte for byte may not be a comic archive at
/// all: `member.format` admits `pdf` and both epub kinds, and a `.cbz` on a PDF
/// would be a lie told in the filename as well as in the `Content-Type`.
/// Spec §7.
pub(crate) fn download_name(
    series: Option<&str>,
    volume: Option<f64>,
    title: &str,
    label: &str,
    extension: &str,
) -> String {
    let stem = safe(series.unwrap_or(title));
    let volume_part = volume.map_or_else(String::new, |v| format!(" - v{}", volume_text(v)));
    format!("{stem}{volume_part} ({}).{extension}", safe(label))
}

/// Two digits at least, the fraction only when there is one.
///
/// `v01`, `v12`, `v105`, `v01.5`. Zero-padded so a downloads folder sorts, and
/// `work.volume` is REAL because omnibus and half-volumes exist.
fn volume_text(volume: f64) -> String {
    let whole = volume.trunc();
    let fraction = volume - whole;
    // `{:02.0}` pads and drops the fraction; a volume past 99 keeps its digits.
    let padded = format!("{whole:02.0}");
    if fraction.abs() < f64::EPSILON {
        padded
    } else {
        let mut tail = format!("{fraction}");
        // `0.5` -> `.5`.
        if let Some(stripped) = tail.strip_prefix('0') {
            tail = stripped.to_string();
        }
        format!("{padded}{tail}")
    }
}

/// A path component nothing can escape from.
///
/// Separators and control characters become `-`; a leading dot goes, so a title
/// cannot make the download a hidden file. A title that arrived from a filename
/// can hold anything a filename can, which is why this is not optional.
fn safe(text: &str) -> String {
    let replaced: String = text
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == ':' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    replaced.trim().trim_start_matches('.').trim().to_string()
}

/// The `Content-Disposition` for a download, in both forms RFC 6266 allows.
///
/// **Both, not one.** This library's titles are routinely non-ASCII —
/// `五等分の花嫁` is the search case that forced migration 8's trigram
/// tokeniser — and a client that understands only the quoted `filename=` must
/// still get something usable, while one that understands `filename*` must get
/// the real name. The quoted form is transliterated to ASCII by dropping what
/// will not fit, which can leave it nearly empty; that is why it is the
/// fallback and not the answer.
pub(crate) fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && c != '"' && c != '\\' && !c.is_control() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::with_capacity(name.len() * 3);
    for byte in name.as_bytes() {
        let c = *byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
            encoded.push(c);
        } else {
            let _ = std::fmt::Write::write_fmt(&mut encoded, format_args!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests;
```

Add `pub(crate) mod download;` to `crates/taiyaku-serve/src/lib.rs`.

- [ ] **Step 4: Run**

Run: `cargo test -p taiyaku-serve download::`
Expected: PASS, nine tests. If `separators_and_controls_cannot_reach_the_filename`
fails on `a-b-c`, check that `safe` maps `\\` — one backslash in the source is
an escape.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/download.rs crates/taiyaku-serve/src/download/tests.rs crates/taiyaku-serve/src/lib.rs
git commit -m "A download's filename, in both the forms RFC 6266 allows, with nothing a title can hide in"
```

---

### Task 7: the edition download — both body shapes, streamed

**Files:**
- Modify: `crates/taiyaku-serve/Cargo.toml`
- Modify: `crates/taiyaku-serve/src/download.rs`
- Modify: `crates/taiyaku-serve/src/download/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (register the route in `guarded_routes`)

**Interfaces:**
- Consumes: `cbz::{Plan, PlannedEntry, clipped}` (Tasks 2–4), `comicinfo::{read, xml}` (Task 5), `download_name`/`content_disposition` (Task 6), `taiyaku_read::{member_page_entries, for_each_member_page, PageEntry}` (Task 1), `routes::refuse_if_hidden` (existing), `ServeState::work_or_missing` (existing).
- Produces: `pub(crate) async fn edition(State<Arc<ServeState>>, AxPath<(String, String)>, HeaderMap, Extensions) -> Response`

- [ ] **Step 1: Add the two dependencies**

In `crates/taiyaku-serve/Cargo.toml`:

```toml
axum = "0.8"
base64 = "0.23.1"
# Both already in the dependency tree via axum and hyper — named directly here
# only so `download.rs` can implement `Stream` over an mpsc receiver and hand
# `Body::from_stream` the `Bytes` it wants. Same versions the tree already
# resolves to, so the resolver unifies one copy rather than two. Same reasoning
# as `rusqlite` below.
bytes = "1.12.1"
futures-core = "0.3.33"
image = "0.25.9"
```

and give `tokio` the `sync` feature for `mpsc`:

```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "sync"] }
```

- [ ] **Step 2: Write the failing tests**

Append to `crates/taiyaku-serve/src/download/tests.rs`:

```rust
/// Read a whole response body, however large the test made it.
async fn all_of(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), 64 << 20)
        .await
        .expect("the body")
        .to_vec()
}

#[tokio::test]
async fn a_two_member_edition_downloads_as_one_readable_cbz() {
    let state = std::sync::Arc::new(crate::test_support::state("download-two-members"));
    let router = crate::test_support::authenticating(crate::build_router(state));

    let response =
        crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(response.status(), 200);

    let length: u64 = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .expect("an exact length before the first byte")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        response.headers().get(axum::http::header::ACCEPT_RANGES).unwrap(),
        "bytes"
    );
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_TYPE).unwrap(),
        "application/vnd.comicbook+zip"
    );

    let bytes = all_of(response).await;
    assert_eq!(bytes.len() as u64, length, "Content-Length was a lie");

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
    // The fixture is two members of two pages each, plus the generated
    // metadata document.
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["ComicInfo.xml", "0001.jpg", "0002.jpg", "0003.jpg", "0004.jpg"],
        "zero-padded and sequential across the whole edition, in reading order"
    );
    // Each fixture entry's bytes are its own original name, so this asserts
    // the *order* as well as the content.
    let mut first = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("0001.jpg").unwrap(), &mut first).unwrap();
    assert_eq!(first, b"a1.jpg");
    let mut third = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("0003.jpg").unwrap(), &mut third).unwrap();
    assert_eq!(third, b"b1.jpg", "member two's first page is page three");

    crate::test_support::cleanup("download-two-members");
}

#[tokio::test]
async fn a_single_member_edition_is_served_byte_for_byte() {
    let (work, fixture) = crate::test_support::work_with_real_pages();
    let on_disk = std::fs::read(fixture.path().join("a1.cbz")).unwrap();
    let state = std::sync::Arc::new(crate::ServeState {
        library: crate::test_support::library(),
        works: vec![work],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = crate::test_support::authenticating(crate::build_router(state));

    let response = crate::test_support::get(&router, "/api/works/w/a/download").await;
    assert_eq!(response.status(), 200);
    let bytes = all_of(response).await;

    assert_eq!(bytes, on_disk, "passthrough must not rebuild");
    // And must not acquire metadata it did not have. This assertion is what
    // stops someone "improving" passthrough into a rebuild.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert!(archive.by_name("ComicInfo.xml").is_err());
}

#[tokio::test]
async fn a_range_of_a_generated_download_is_that_slice_of_the_whole() {
    let state = std::sync::Arc::new(crate::test_support::state("download-range"));
    let router = crate::test_support::authenticating(crate::build_router(state));

    let whole = all_of(crate::test_support::get(&router, "/api/works/t/raw/download").await).await;

    use tower::ServiceExt as _;
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::get("/api/works/t/raw/download")
                .header(axum::http::header::RANGE, "bytes=10-49")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 206);
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_RANGE).unwrap(),
        &format!("bytes 10-49/{}", whole.len())
    );
    let part = all_of(response).await;
    assert_eq!(part, whole[10..50], "the range is not that slice of the file");

    crate::test_support::cleanup("download-range");
}

#[tokio::test]
async fn an_unsatisfiable_range_is_refused_with_the_real_length() {
    let state = std::sync::Arc::new(crate::test_support::state("download-bad-range"));
    let router = crate::test_support::authenticating(crate::build_router(state));

    use tower::ServiceExt as _;
    let response = router
        .oneshot(
            axum::http::Request::get("/api/works/t/raw/download")
                .header(axum::http::header::RANGE, "bytes=99999999-")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 416);
    let stated = response
        .headers()
        .get(axum::http::header::CONTENT_RANGE)
        .expect("416 must say how long the thing actually is")
        .to_str()
        .unwrap()
        .to_string();
    assert!(stated.starts_with("bytes */"), "got {stated}");

    crate::test_support::cleanup("download-bad-range");
}

#[tokio::test]
async fn the_filename_comes_from_the_library_rows_when_it_has_them() {
    let (router, _state, _fixture) =
        crate::test_support::router_over_a_scanned_work_in_a_series();
    let router = crate::test_support::authenticating(router);

    // The fixture's slug and edition key come from the helper; find them the
    // way a client would, through the browse route.
    let listed = crate::test_support::body_string(
        crate::test_support::get(&router, "/api/library/series/1").await,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&listed).unwrap();
    let slug = value["works"][0]["slug"].as_str().unwrap().to_string();
    let key = value["works"][0]["editions"][0]["key"].as_str().unwrap().to_string();

    let response =
        crate::test_support::get(&router, &format!("/api/works/{slug}/{key}/download")).await;
    assert_eq!(response.status(), 200);
    let disposition = response
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .expect("a download names itself")
        .to_str()
        .unwrap()
        .to_string();
    assert!(disposition.contains("Scanned - v01"), "got {disposition}");
    assert!(disposition.contains(".cbz"), "got {disposition}");
}

#[tokio::test]
async fn an_edition_that_does_not_exist_is_a_404_before_any_bytes() {
    let state = std::sync::Arc::new(crate::test_support::state("download-no-edition"));
    let router = crate::test_support::authenticating(crate::build_router(state));

    let response =
        crate::test_support::get(&router, "/api/works/t/nonesuch/download").await;
    assert_eq!(response.status(), 404);
    // A failure a stream cannot report must happen before the stream starts —
    // spec §5.
    assert!(response.headers().get(axum::http::header::CONTENT_LENGTH).is_some());

    crate::test_support::cleanup("download-no-edition");
}
```

- [ ] **Step 3: Run and watch fail**

Run: `cargo test -p taiyaku-serve download::`
Expected: FAIL — every new test 404s, because `/api/works/{work}/{edition}/download`
is not a route yet.

- [ ] **Step 4: Implement the handler**

Append to `crates/taiyaku-serve/src/download.rs`. Read the module doc comment
you wrote in Task 6 first; this is the second half of the same file.

```rust
use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;

use crate::{ServeState, cbz};

/// The registered type for a comic archive, and what an OPDS client matches
/// its acquisition link against.
pub(crate) const CBZ_TYPE: &str = "application/vnd.comicbook+zip";

/// What a passthrough download says it is.
///
/// **The type follows the bytes, not the route.** A single-member edition may
/// not be a comic archive at all — `member.format` admits `pdf` and both epub
/// kinds — and calling a PDF `application/vnd.comicbook+zip` would be a lie
/// told in the header as well as in the filename. Spec §7. Only the *generated*
/// path is unconditionally [`CBZ_TYPE`], because there the container is this
/// module's own.
fn type_for(extension: &str) -> &'static str {
    match extension {
        "cbz" | "zip" => CBZ_TYPE,
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "cbr" | "rar" => "application/vnd.comicbook-rar",
        _ => "application/octet-stream",
    }
}

/// How much unsent output the producer may run ahead by, in chunks.
///
/// The channel is the backpressure: a bounded sender blocks the blocking
/// thread when the client is slower than the disk, which is what keeps a
/// 1.49 GB volume out of memory. Eight is enough that a fast client never
/// waits on a page read and small enough that the high-water mark is eight
/// pages rather than a volume — and this corpus's pages run 100 KB–3 MB, with
/// ~430 MB outliers in the Color Walk art books (`taiyaku-read`'s
/// `MAX_ENTRY_CAPACITY_HINT`).
const CHANNEL_DEPTH: usize = 8;

/// An `mpsc` receiver as the `Stream` `Body::from_stream` wants.
///
/// Hand-written rather than pulling in `tokio-stream`: the workspace's
/// dependency set is deliberately short enough to quote in `docs/status.md`,
/// and this is the whole of what that crate would be used for here.
struct ChannelStream(tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>);

impl futures_core::Stream for ChannelStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

/// What the route decided to send, before it sends any of it.
enum Shape {
    /// One member's own bytes, from this absolute path.
    Passthrough {
        path: std::path::PathBuf,
        length: u64,
        content_type: &'static str,
    },
    /// A `STORE` zip built from these members, in this order.
    Generated {
        plan: cbz::Plan,
        /// One `(absolute member path, how many of the plan's entries it
        /// contributes)`, in order. The metadata document is entry 0 and is
        /// not a member.
        members: Vec<std::path::PathBuf>,
        comic_info: Vec<u8>,
    },
}

/// `GET /api/works/{slug}/{edition}/download`.
///
/// Spec C2 §2, §3, §5. The order of the two guards is the design: the root
/// grant answers 404 because an edition outside the grant does not admit that
/// it exists, and `can_download` answers 403 because "you may see this and may
/// not have it" is a different fact that a 404 would lie about.
pub(crate) async fn edition(
    State(state): State<Arc<ServeState>>,
    AxPath((slug, edition_key)): AxPath<(String, String)>,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) =
        crate::routes::refuse_if_hidden(&state, &extensions, &slug, &edition_key)
    {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_download(&extensions) {
        return refusal;
    }

    // Everything that can fail happens here, on a blocking thread, before the
    // body exists — because once the body has begun a failure cannot become a
    // status code. Spec §5.
    let prepared = {
        let state = state.clone();
        let slug = slug.clone();
        let edition_key = edition_key.clone();
        tokio::task::spawn_blocking(move || prepare(&state, &slug, &edition_key)).await
    };
    let (shape, name) = match prepared {
        Ok(Ok(pair)) => pair,
        Ok(Err(refusal)) => return refusal,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("preparing the download panicked: {e}"),
            )
                .into_response();
        }
    };

    let total = match &shape {
        Shape::Passthrough { length, .. } => *length,
        Shape::Generated { plan, .. } => plan.len(),
    };
    let content_type = match &shape {
        Shape::Passthrough { content_type, .. } => *content_type,
        Shape::Generated { .. } => CBZ_TYPE,
    };

    let (from, upto, partial) = match requested_range(&headers, total) {
        Ok(triple) => triple,
        Err(()) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{total}"))],
                "that range is not inside this file",
            )
                .into_response();
        }
    };

    let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_DEPTH);
    std::thread::Builder::new()
        .name("taiyaku-download".to_string())
        .spawn(move || produce(shape, from, upto, &tx))
        .expect("spawning the download producer");

    streamed(rx, Sent { from, upto, total, partial, content_type, name })
}

/// What a download's headers state, gathered so the two handlers say it once.
struct Sent {
    from: u64,
    upto: u64,
    total: u64,
    partial: bool,
    content_type: &'static str,
    name: String,
}

/// The response both handlers return: a streamed body and the headers that
/// describe it.
///
/// One function rather than one per handler. The edition download and the
/// member download differ in *what they produce* and in nothing about how they
/// announce it, and two copies of this block would be two places for a
/// `Content-Range` off-by-one to be fixed in one of.
fn streamed(
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
    sent: Sent,
) -> Response {
    let mut response = axum::body::Body::from_stream(ChannelStream(rx)).into_response();
    *response.status_mut() = if sent.partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        sent.content_type.parse().expect("static type"),
    );
    h.insert(
        header::CONTENT_LENGTH,
        (sent.upto - sent.from).to_string().parse().expect("a number"),
    );
    h.insert(header::ACCEPT_RANGES, "bytes".parse().expect("static"));
    h.insert(
        header::CONTENT_DISPOSITION,
        content_disposition(&sent.name)
            .parse()
            .unwrap_or_else(|_| "attachment".parse().expect("static")),
    );
    if sent.partial {
        h.insert(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", sent.from, sent.upto - 1, sent.total)
                .parse()
                .expect("a range"),
        );
    }
    // A download is one file, not a per-reader document — but it is still
    // per-*grant*, so `private` stays and only `no-store` goes: a client may
    // keep the volume it just fetched.
    h.insert(
        header::CACHE_CONTROL,
        "private, no-cache".parse().expect("static"),
    );
    response
}

/// Decide the shape and the name, opening whatever has to be opened.
///
/// Runs on a blocking thread. Every refusal this can produce is a complete
/// `Response`, because all of them must be sent instead of a body rather than
/// in the middle of one.
fn prepare(
    state: &Arc<ServeState>,
    slug: &str,
    edition_key: &str,
) -> Result<(Shape, String), Response> {
    let work = state
        .work_or_missing(slug)
        .map_err(crate::routes::not_found)?;
    let sidecar = work.sidecar();
    let found = sidecar.editions.get(edition_key).ok_or_else(|| {
        crate::routes::not_found(format!("no edition `{edition_key}` in `{slug}`"))
    })?;
    let root = work
        .path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    // A scanned work has no document, so its member paths are absolute and
    // this join is the identity — `derive.rs` says so where it writes them.
    let paths: Vec<std::path::PathBuf> =
        found.members.iter().map(|m| root.join(&m.path)).collect();
    if paths.is_empty() {
        return Err(crate::routes::not_found(format!(
            "`{edition_key}` of `{slug}` has no members"
        )));
    }

    // The library rows the name and the metadata document come from. A work
    // this server holds only as a document has no rows, and the fallbacks are
    // what that case gets.
    let (series, volume, title) = {
        let conn = state.library.conn();
        conn.query_row(
            "SELECT
               (SELECT COALESCE(
                  (SELECT title FROM series_title
                    WHERE series_id = w.series_id ORDER BY is_primary DESC, title LIMIT 1),
                  (SELECT sort_title FROM series WHERE id = w.series_id))),
               w.volume, w.title
             FROM work w WHERE w.slug = ?1",
            [slug],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .unwrap_or((None, None, sidecar.work.title.clone()))
    };

    let single_file = paths.len() == 1 && paths[0].is_file();
    if single_file {
        let length = std::fs::metadata(&paths[0])
            .map_err(|e| {
                crate::routes::not_found(format!("{}: {e}", paths[0].display()))
            })?
            .len();
        let extension = paths[0]
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("cbz")
            .to_ascii_lowercase();
        let content_type = type_for(&extension);
        let name = download_name(
            series.as_deref(),
            volume,
            &title,
            &found.label,
            &extension,
        );
        return Ok((
            Shape::Passthrough {
                path: paths[0].clone(),
                length,
                content_type,
            },
            name,
        ));
    }

    // Generated. Describe every page first — for an archive member this reads
    // only central directories.
    let mut described: Vec<taiyaku_read::PageEntry> = Vec::new();
    for path in &paths {
        let entries = taiyaku_read::member_page_entries(path)
            .map_err(crate::routes::not_found)?;
        described.extend(entries);
    }
    if described.is_empty() {
        return Err(crate::routes::not_found(format!(
            "`{edition_key}` of `{slug}` holds no pages"
        )));
    }

    let page_count = u32::try_from(described.len()).unwrap_or(u32::MAX);
    let comic_info = {
        let conn = state.library.conn();
        crate::comicinfo::read(&conn, slug, edition_key, page_count)
            .map(|info| crate::comicinfo::xml(&info).into_bytes())
            .unwrap_or_else(|_| {
                // No rows for this work — a document-only library. The
                // document still states what the sidecar knows.
                crate::comicinfo::xml(&crate::comicinfo::ComicInfo {
                    series: None,
                    number: None,
                    title: Some(title.clone()),
                    page_count,
                    language: None,
                    credits: Vec::new(),
                    notes: Some(found.label.clone()),
                })
                .into_bytes()
            })
    };

    // Zero-padded and sequential across the whole edition, width from the
    // total, extension from the source entry. Name order is then reading
    // order, which is the trap `docs/sidecar.md`'s rule 1 exists to document.
    let width = page_count.to_string().len().max(4);
    let mut planned = vec![cbz::PlannedEntry {
        name: "ComicInfo.xml".to_string(),
        size: comic_info.len() as u64,
        crc32: crc32fast::hash(&comic_info),
    }];
    for (index, entry) in described.iter().enumerate() {
        let extension = std::path::Path::new(&entry.name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg")
            .to_ascii_lowercase();
        planned.push(cbz::PlannedEntry {
            name: format!("{:0width$}.{extension}", index + 1, width = width),
            size: entry.size,
            crc32: entry.crc32,
        });
    }

    let name = download_name(series.as_deref(), volume, &title, &found.label, "cbz");
    Ok((
        Shape::Generated {
            plan: cbz::Plan::of(planned),
            members: paths,
            comic_info,
        },
        name,
    ))
}

/// `(from, upto, is a 206)`, or `Err(())` for a range outside the file.
///
/// One range only. A multi-range request is answered with the whole file,
/// which RFC 9110 explicitly permits and which is the right trade here: the
/// `multipart/byteranges` body a real answer needs is not something any OPDS
/// client has been observed asking for, and inventing it unmeasured is the
/// mistake spec §5 avoids elsewhere.
fn requested_range(headers: &HeaderMap, total: u64) -> Result<(u64, u64, bool), ()> {
    let Some(value) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return Ok((0, total, false));
    };
    let Some(spec) = value.strip_prefix("bytes=") else {
        return Ok((0, total, false));
    };
    if spec.contains(',') {
        return Ok((0, total, false));
    }
    let (start, end) = spec.split_once('-').ok_or(())?;
    let (from, upto) = match (start.trim(), end.trim()) {
        ("", "") => return Err(()),
        // `bytes=-500`: the last 500 bytes.
        ("", tail) => {
            let n: u64 = tail.parse().map_err(|_| ())?;
            (total.saturating_sub(n), total)
        }
        (head, "") => (head.parse().map_err(|_| ())?, total),
        (head, tail) => {
            let from: u64 = head.parse().map_err(|_| ())?;
            let last: u64 = tail.parse().map_err(|_| ())?;
            (from, last.saturating_add(1).min(total))
        }
    };
    if from >= total || from >= upto {
        return Err(());
    }
    Ok((from, upto, true))
}

/// Send `[from, upto)` of the output down `tx`, then stop.
///
/// Runs on its own thread, not a `spawn_blocking` worker: this holds the
/// thread for the whole download, which for a 1.49 GB volume over a slow
/// connection is minutes, and tokio's blocking pool is sized for short waits.
fn produce(
    shape: Shape,
    from: u64,
    upto: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) {
    let sent = match shape {
        Shape::Passthrough { path, .. } => passthrough(&path, from, upto, tx),
        Shape::Generated { plan, members, comic_info } => {
            generated(&plan, &members, &comic_info, from, upto, tx)
        }
    };
    if let Err(message) = sent {
        // The body has already begun, so this cannot be a status code. Closing
        // the stream short of `Content-Length` is the only signal available;
        // the log line is what makes it diagnosable. Spec §5.
        eprintln!("taiyaku: download failed mid-stream: {message}");
        let _ = tx.blocking_send(Err(std::io::Error::other(message)));
    }
}

/// One member's own bytes, seeking to `from`.
fn passthrough(
    path: &std::path::Path,
    from: u64,
    upto: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Result<(), String> {
    use std::io::{Read as _, Seek as _};
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.seek(std::io::SeekFrom::Start(from))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut left = upto - from;
    let mut buffer = vec![0u8; 64 * 1024];
    while left > 0 {
        let want = usize::try_from(left).unwrap_or(usize::MAX).min(buffer.len());
        let read = file
            .read(&mut buffer[..want])
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if read == 0 {
            return Err(format!("{}: ended early", path.display()));
        }
        if tx
            .blocking_send(Ok(Bytes::copy_from_slice(&buffer[..read])))
            .is_err()
        {
            // The client hung up. Not an error; stop reading.
            return Ok(());
        }
        left -= read as u64;
    }
    Ok(())
}

/// The generated zip, produced from zero and clipped to `[from, upto)`.
fn generated(
    plan: &cbz::Plan,
    members: &[std::path::PathBuf],
    comic_info: &[u8],
    from: u64,
    upto: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Result<(), String> {
    let mut at = 0u64;
    let mut index = 0usize;
    // `false` once the client has gone.
    let mut send = |bytes: &[u8], at: u64| -> Result<bool, String> {
        let part = cbz::clipped(bytes, at, from, upto);
        if part.is_empty() {
            return Ok(true);
        }
        Ok(tx.blocking_send(Ok(Bytes::copy_from_slice(part))).is_ok())
    };

    // Entry 0 is the metadata document.
    let header = plan.local_header(index);
    if !send(&header, at)? {
        return Ok(());
    }
    at += header.len() as u64;
    if !send(comic_info, at)? {
        return Ok(());
    }
    at += comic_info.len() as u64;
    index += 1;

    for path in members {
        // The plan is authoritative about names and lengths; the member is
        // asked only for bytes. A mismatch means the archive changed since
        // `prepare` read its central directory, and continuing would produce
        // a file whose own directory lies.
        let mut failed: Option<String> = None;
        let mut gone = false;
        let visited = taiyaku_read::for_each_member_page(path, &mut |entry, bytes| {
            let planned = plan.entries().get(index).ok_or_else(|| {
                format!("{}: more pages than the plan named", path.display())
            })?;
            if planned.size != entry.size || planned.crc32 != entry.crc32 {
                // **Two causes, one refusal, and the message must not pick
                // one.** The plan's size is what the archive's central
                // directory *declared*; `entry.size` is what
                // `for_each_member_page` actually read. They differ either
                // because the archive was replaced since `prepare` read it,
                // or because it declares a size it never had — `zip`
                // validates an entry's crc32 against its bytes and never
                // validates its declared length, so a size-lying archive
                // reaches here with a perfectly good crc. Either way the
                // local header already sent states a length these bytes do
                // not have, so the only safe move is to stop.
                return Err(format!(
                    "{}: `{}` is not the {} bytes this download was planned around",
                    path.display(),
                    entry.name,
                    planned.size
                ));
            }
            let header = plan.local_header(index);
            match send(&header, at) {
                Ok(true) => {}
                Ok(false) => {
                    gone = true;
                    return Err(String::new());
                }
                Err(e) => return Err(e),
            }
            at += header.len() as u64;
            match send(bytes, at) {
                Ok(true) => {}
                Ok(false) => {
                    gone = true;
                    return Err(String::new());
                }
                Err(e) => return Err(e),
            }
            at += bytes.len() as u64;
            index += 1;
            // Nothing after `upto` needs producing.
            if at >= upto {
                gone = true;
                return Err(String::new());
            }
            Ok(())
        });
        if let Err(message) = visited {
            if gone {
                return Ok(());
            }
            failed = Some(message);
        }
        if let Some(message) = failed {
            return Err(message);
        }
    }

    let trailer = plan.trailer();
    let _ = send(&trailer, at)?;
    Ok(())
}
```

Add `crc32fast = "1.5.0"` to `crates/taiyaku-serve/Cargo.toml` as well — the
plan hashes the generated `ComicInfo.xml`, whose bytes exist only here — with
the same "already in the tree, named directly" comment.

Make `refuse_if_hidden` and `not_found` reachable: in
`crates/taiyaku-serve/src/routes.rs`, change `fn refuse_if_hidden(` to
`pub(crate) fn refuse_if_hidden(` and `fn not_found(` likewise.

- [ ] **Step 5: Register the route**

In `routes::guarded_routes`, in the first `vec![` beside the page routes:

```rust
        (
            "/api/works/{work}/{edition}/download",
            get(crate::download::edition),
        ),
```

- [ ] **Step 6: Run**

Run: `cargo test -p taiyaku-serve download::`
Expected: PASS. `every_route_refuses_an_anonymous_request` will now FAIL on its
count — Task 9 fixes it, and leaving it red until then is the point: the count
is what proves the route arrived guarded.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/Cargo.toml crates/taiyaku-serve/src/download.rs crates/taiyaku-serve/src/download/tests.rs crates/taiyaku-serve/src/routes.rs Cargo.lock
git commit -m "An edition downloads: byte-for-byte when it is one file, a streamed STORE zip when it is not"
```

---

### Task 8: the member download

**Files:**
- Modify: `crates/taiyaku-serve/src/download.rs`
- Modify: `crates/taiyaku-serve/src/download/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs`

**Interfaces:**
- Consumes: `passthrough`, `requested_range`, `streamed`, `Sent`, `type_for`, `CHANNEL_DEPTH`, `ChannelStream` — all from Task 7. **Nothing in this task re-states a header block or a content-type match**; both live once in `download.rs` already.
- Produces: `pub(crate) async fn member(State<Arc<ServeState>>, AxPath<(String, String, String)>, HeaderMap, Extensions) -> Response`

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/download/tests.rs`:

```rust
#[tokio::test]
async fn a_member_downloads_as_the_file_on_disk() {
    let state = std::sync::Arc::new(crate::test_support::state("download-member"));
    let root = std::env::temp_dir().join("download-member");
    let second = std::fs::read(root.join("c2.cbz")).unwrap();
    let router = crate::test_support::authenticating(crate::build_router(state));

    // Ordinal 1 is the second member — the document's order, counting from 0.
    let response =
        crate::test_support::get(&router, "/api/works/t/raw/members/1/download").await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_TYPE).unwrap(),
        "application/vnd.comicbook+zip"
    );
    let disposition = response
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(disposition.contains("c2.cbz"), "a member keeps its own name: {disposition}");
    assert_eq!(all_of(response).await, second);

    crate::test_support::cleanup("download-member");
}

#[tokio::test]
async fn a_member_ordinal_past_the_end_is_a_404() {
    let state = std::sync::Arc::new(crate::test_support::state("download-member-past"));
    let router = crate::test_support::authenticating(crate::build_router(state));

    let response =
        crate::test_support::get(&router, "/api/works/t/raw/members/9/download").await;
    assert_eq!(response.status(), 404);
    let body = crate::test_support::body_string(response).await;
    assert!(body.contains("2 members"), "say how many there are: {body}");

    crate::test_support::cleanup("download-member-past");
}

#[tokio::test]
async fn a_member_range_is_that_slice_of_the_file() {
    let state = std::sync::Arc::new(crate::test_support::state("download-member-range"));
    let root = std::env::temp_dir().join("download-member-range");
    let first = std::fs::read(root.join("c1.cbz")).unwrap();
    let router = crate::test_support::authenticating(crate::build_router(state));

    use tower::ServiceExt as _;
    let response = router
        .oneshot(
            axum::http::Request::get("/api/works/t/raw/members/0/download")
                .header(axum::http::header::RANGE, "bytes=5-14")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 206);
    assert_eq!(all_of(response).await, first[5..15]);

    crate::test_support::cleanup("download-member-range");
}
```

- [ ] **Step 2: Run and watch fail**

Run: `cargo test -p taiyaku-serve download::tests::a_member`
Expected: FAIL — 404 from the router's fallback, because the route does not
exist.

- [ ] **Step 3: Implement**

Append to `crates/taiyaku-serve/src/download.rs`:

```rust
/// `GET /api/works/{slug}/{edition}/members/{ordinal}/download`.
///
/// One member's original bytes, unchanged. `{ordinal}` is the position in the
/// document counting from 0 — `member.ordinal`, which `import.rs` writes and
/// `Edition::locate` returns. **Not the path**: a path is long, holds
/// separators, and is the field `import` reconciles on, so a URL built from one
/// would move under a rescan that followed the content identity rule as
/// designed.
///
/// There is no OPDS mirror of this route. OPDS asks for a publication, and a
/// chapter file inside one is not something a feed has an entry for.
pub(crate) async fn member(
    State(state): State<Arc<ServeState>>,
    AxPath((slug, edition_key, raw)): AxPath<(String, String, String)>,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) =
        crate::routes::refuse_if_hidden(&state, &extensions, &slug, &edition_key)
    {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_download(&extensions) {
        return refusal;
    }
    // Refused here in this server's own words rather than by axum's `Path`
    // rejection in terms of a Rust type — the same reason `page_index` exists
    // (O5).
    let Ok(ordinal) = raw.parse::<usize>() else {
        return crate::routes::not_found(format!(
            "`{raw}` is not a member number; they count from 0"
        ));
    };

    let prepared = {
        let state = state.clone();
        let slug = slug.clone();
        let edition_key = edition_key.clone();
        tokio::task::spawn_blocking(move || prepare_member(&state, &slug, &edition_key, ordinal))
            .await
    };
    let (path, length, content_type, name) = match prepared {
        Ok(Ok(four)) => four,
        Ok(Err(refusal)) => return refusal,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("preparing the member download panicked: {e}"),
            )
                .into_response();
        }
    };

    let (from, upto, partial) = match requested_range(&headers, length) {
        Ok(triple) => triple,
        Err(()) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{length}"))],
                "that range is not inside this file",
            )
                .into_response();
        }
    };

    let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_DEPTH);
    std::thread::Builder::new()
        .name("taiyaku-download-member".to_string())
        .spawn(move || {
            if let Err(message) = passthrough(&path, from, upto, &tx) {
                eprintln!("taiyaku: member download failed mid-stream: {message}");
                let _ = tx.blocking_send(Err(std::io::Error::other(message)));
            }
        })
        .expect("spawning the member download producer");

    streamed(
        rx,
        Sent { from, upto, total: length, partial, content_type, name },
    )
}

/// `(path, length, content type, filename)` for one member.
fn prepare_member(
    state: &Arc<ServeState>,
    slug: &str,
    edition_key: &str,
    ordinal: usize,
) -> Result<(std::path::PathBuf, u64, &'static str, String), Response> {
    let work = state
        .work_or_missing(slug)
        .map_err(crate::routes::not_found)?;
    let sidecar = work.sidecar();
    let found = sidecar.editions.get(edition_key).ok_or_else(|| {
        crate::routes::not_found(format!("no edition `{edition_key}` in `{slug}`"))
    })?;
    let held = found.members.get(ordinal).ok_or_else(|| {
        crate::routes::not_found(format!(
            "`{edition_key}` of `{slug}` has {} members; there is no member {ordinal}",
            found.members.len()
        ))
    })?;
    let root = work.path.parent().unwrap_or(std::path::Path::new("."));
    let path = root.join(&held.path);
    if path.is_dir() {
        return Err(crate::routes::not_found(format!(
            "member {ordinal} of `{edition_key}` is a directory of pages, not a file; download the edition instead"
        )));
    }
    let length = std::fs::metadata(&path)
        .map_err(|e| crate::routes::not_found(format!("{}: {e}", path.display())))?
        .len();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("cbz")
        .to_ascii_lowercase();
    let content_type = type_for(&extension);
    // **A member keeps its own filename.** The edition download gets a name
    // built from the library's rows because it is a file that did not exist
    // before; a member already had one, and it is the name the person who
    // packed it chose.
    let name = path
        .file_name()
        .map_or_else(|| format!("member-{ordinal}.{extension}"), |n| {
            n.to_string_lossy().to_string()
        });
    Ok((path, length, content_type, name))
}
```

- [ ] **Step 4: Register the route**

In `routes::guarded_routes`, beside the edition download:

```rust
        (
            "/api/works/{work}/{edition}/members/{ordinal}/download",
            get(crate::download::member),
        ),
```

- [ ] **Step 5: Run**

Run: `cargo test -p taiyaku-serve download::`
Expected: PASS on everything except the route-count test, which Task 9 fixes.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/download.rs crates/taiyaku-serve/src/download/tests.rs crates/taiyaku-serve/src/routes.rs
git commit -m "A member downloads as itself, addressed by ordinal and keeping the name whoever packed it chose"
```

---

### Task 9: the two guards, and what each one is for

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs` (two helpers beside `refuse_if_hidden`; `get_page` gains one)
- Modify: `crates/taiyaku-serve/src/opds.rs` (`stream_page` gains one)
- Modify: `crates/taiyaku-serve/src/auth/tests.rs` (the enumeration count)
- Modify: `crates/taiyaku-serve/src/download/tests.rs`

**Interfaces:**
- Consumes: `auth::Principal` from the request extensions, as `refuse_if_hidden` already does.
- Produces:
  - `pub(crate) fn refuse_if_cannot_download(extensions: &Extensions) -> Option<Response>`
  - `pub(crate) fn refuse_if_cannot_stream(extensions: &Extensions) -> Option<Response>`

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/download/tests.rs`:

```rust
/// Turn one capability off on the fixture account.
fn withhold(state: &crate::ServeState, column: &str) {
    let conn = state.library.conn();
    conn.execute(&format!("UPDATE user SET {column} = 0"), [])
        .expect("withholds the capability");
}

#[tokio::test]
async fn without_can_download_the_download_is_403_and_the_shelf_still_works() {
    let (router, state) = crate::test_support::router_over_a_library().await;
    withhold(&state, "can_download");
    let router = crate::test_support::authenticating(router);

    // 403, not 404: the caller may see this edition and may not have it. A 404
    // here would be a lie that also breaks the UI's ability to explain itself.
    for path in [
        "/api/works/t/raw/download",
        "/api/works/t/raw/members/0/download",
    ] {
        let response = crate::test_support::get(&router, path).await;
        assert_eq!(response.status(), 403, "{path}");
    }

    // Everything a download-only refusal must not take away.
    for path in [
        "/api/library/series",
        "/api/works/t/raw/pages/0/thumb",
    ] {
        let response = crate::test_support::get(&router, path).await;
        assert_ne!(response.status(), 403, "{path} is how the shelf renders");
    }
}

#[tokio::test]
async fn without_can_stream_the_pages_are_403_and_the_download_still_works() {
    let (router, state) = crate::test_support::router_over_a_library().await;
    withhold(&state, "can_stream");
    let router = crate::test_support::authenticating(router);

    for path in [
        "/api/works/t/raw/pages/0",
        "/opds/v1.2/works/t/raw/pages/0",
    ] {
        let response = crate::test_support::get(&router, path).await;
        assert_eq!(response.status(), 403, "{path}");
    }

    // The coherent half of the policy: take the volume away, not the shelf.
    let response = crate::test_support::get(&router, "/api/works/t/raw/pages/0/thumb").await;
    assert_ne!(response.status(), 403, "a thumbnail is how the shelf renders");
    let response = crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(response.status(), 200, "can_stream must not gate a download");
}
```

If `router_over_a_library`'s work is not slugged `t` or its edition not `raw`,
read the helper at `test_support.rs:396` and use the names it actually creates
rather than changing the helper.

- [ ] **Step 2: Run and watch fail**

Run: `cargo test -p taiyaku-serve download::tests::without_can`
Expected: FAIL — both get 200 where 403 is asserted, because nothing reads
either flag yet. This failure **is** O54.

- [ ] **Step 3: Implement the two helpers**

> **`refuse_if_cannot_download` IS ALREADY IMPLEMENTED — write only
> `refuse_if_cannot_stream` here.** Task 7 landed it as an always-passing stub
> so the crate would compile; a background security review flagged the
> fail-open gate, and it was replaced with the real check one commit later,
> along with `download::tests::a_withheld_download_capability_refuses_with_403`
> (proven red against the stub first). Do not add a second function and do not
> re-stub it. What remains of this task: `refuse_if_cannot_stream`, the
> remaining refusal tests, and the route count.

In `crates/taiyaku-serve/src/routes.rs`, immediately after `refuse_if_hidden`:

```rust
/// **403, not 404, and that is the design.** `refuse_if_hidden` above answers
/// 404 because an edition outside the grant does not admit that it exists;
/// this answers 403 because the caller *may* see this edition and may not have
/// a copy of it. Two different facts, two different codes — and a 404 here
/// would be a lie the UI could not explain to the person reading it.
///
/// Spec C2 §2.1. This function and [`refuse_if_cannot_stream`] are what close
/// the open half of O54: `can_download` and `can_stream` had been resolved on
/// every request since spec A2 and governed nothing.
pub(crate) fn refuse_if_cannot_download(
    extensions: &axum::http::Extensions,
) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    (!principal.can_download).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may browse and read but not download",
        )
            .into_response()
    })
}

/// The same for reading pages in place.
///
/// **Gates `/pages` and the OPDS PSE stream, and deliberately not `/thumb`, the
/// library covers, or any browse route.** The line is there because a
/// download-only account must still be able to *see* its shelf: an account that
/// cannot render a cover cannot choose what to download, and "you may take
/// volumes away but not read them here" is a coherent policy while "you may
/// take volumes away but not see which ones exist" is not. Thumbnails are how
/// the shelf renders, so they follow browse.
///
/// Both columns default to 1 (`schema/004_auth.sql:18`), so nothing changed for
/// any existing account the day this shipped, and nothing can set either to 0
/// outside a fixture until spec C4's user administration. Spec C2 §2.2.
pub(crate) fn refuse_if_cannot_stream(
    extensions: &axum::http::Extensions,
) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    (!principal.can_stream).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may browse and download but not read pages here",
        )
            .into_response()
    })
}
```

- [ ] **Step 4: Call the stream guard from both page routes**

In `routes::get_page`, immediately after the existing `refuse_if_hidden` call
(around line 486):

```rust
    if let Some(refusal) = refuse_if_cannot_stream(&request_extensions) {
        return refusal;
    }
```

`get_thumb` does **not** get this — read the doc comment on
`refuse_if_cannot_stream` before deciding otherwise.

In `opds::stream_page`, the handler needs the extensions to ask. Add
`extensions: axum::http::Extensions` to its parameter list (axum supplies it
from any handler position) and, after the existing hidden check, the same two
lines calling `crate::routes::refuse_if_cannot_stream(&extensions)`.

- [ ] **Step 5: Update the enumeration count**

Run: `cargo test -p taiyaku-serve every_route_refuses_an_anonymous_request`

It fails with the two counts. Read the test at
`crates/taiyaku-serve/src/auth/tests.rs`, find the expected total, and raise it
by **two** — the edition download and the member download. Both are in
`guarded_routes`, so the test's own walk covers them; only the number needs
saying. Do not touch `open_routes`.

- [ ] **Step 6: Run the whole crate**

Run: `cargo test -p taiyaku-serve`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/opds.rs crates/taiyaku-serve/src/auth/tests.rs crates/taiyaku-serve/src/download/tests.rs
git commit -m "O54's other half: can_download gates the downloads, can_stream gates the pages, and a thumbnail is how the shelf renders"
```

---

### Task 10: two corpus standing checks

**Files:**
- Modify: `crates/taiyaku-serve/src/corpus.rs`

**Interfaces:**
- Consumes: the finished routes; whatever fixture pattern `corpus.rs` already
  uses to reach real material. **Read the top of that file first** — it has an
  established way of skipping when `TAIYAKU_CORPUS` is unset, and adding a
  second way is the drift O30 closed.

**Why these exist:** a fixture archive is two entries of six bytes, written by
the `zip` crate with default options. Real material is 1.49 GB, `DEFLATE`, with
nested page folders and a `ComicInfo.xml` already inside. The generated path's
whole job is to survive the second, and only the corpus job can say whether it
does.

- [ ] **Step 1: Write the failing checks**

Append to `crates/taiyaku-serve/src/corpus.rs`, in the shape the file already
uses:

```rust
/// A real multi-member edition downloads as a zip that reads back with the
/// page count the edition claims.
///
/// The fixtures cannot reach this: they are `Stored` entries of six bytes, and
/// 11,431 of this corpus's 11,681 entries are `DEFLATE`, so a bug in the
/// inflate-then-store path would be invisible to every other test in this
/// crate.
#[test]
fn a_real_multi_member_edition_downloads_as_a_readable_cbz() {
    let Some(corpus) = corpus_root() else {
        return;
    };
    // Whichever corpus edition has more than one member. `gotoubun-v01` holds
    // chapter files; if that stops being true, this check reports it rather
    // than silently testing the single-member path twice.
    let (router, slug, key, pages) = a_corpus_edition_with_members(&corpus, 2..);

    let response = block_on(download(&router, &slug, &key));
    assert_eq!(response.status(), 200);
    let stated: u64 = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .expect("an exact length")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let bytes = block_on(all_of(response));
    assert_eq!(bytes.len() as u64, stated, "Content-Length was a lie on real material");

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a readable zip");
    // Every page, plus the generated metadata document.
    assert_eq!(archive.len() as u32, pages + 1);
    assert_eq!(archive.by_index(0).unwrap().name(), "ComicInfo.xml");
    // Names sort into reading order, which is the whole point of renumbering
    // them — `docs/sidecar.md` rule 1.
    let names: Vec<String> = (1..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "stored order must already be name order");
}

/// A real single-member edition is served byte for byte.
#[test]
fn a_real_single_member_edition_is_the_file_on_disk() {
    let Some(corpus) = corpus_root() else {
        return;
    };
    let (router, slug, key, path) = a_corpus_edition_of_one_file(&corpus);
    let on_disk = std::fs::read(&path).expect("the archive");

    let response = block_on(download(&router, &slug, &key));
    assert_eq!(response.status(), 200);
    let bytes = block_on(all_of(response));
    assert_eq!(bytes.len(), on_disk.len());
    assert_eq!(bytes, on_disk, "passthrough must not rebuild real material either");
}
```

The four helpers this needs — `corpus_root`, `a_corpus_edition_with_members`,
`a_corpus_edition_of_one_file`, `download`, `all_of`, `block_on` — are to be
written **in the idiom `corpus.rs` already has**. Read the file: it already
builds a library over the corpus and already decides how to skip. Reuse those;
write only what is genuinely missing, and if the file already has a
`block_on`-equivalent or is `#[tokio::test]` throughout, follow that instead of
introducing a second async style.

- [ ] **Step 2: Run the corpus job**

```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-serve --all-features corpus:: -- --nocapture --test-threads=1
```

Expected: FAIL first — the helpers do not exist. Then, once they do, PASS. Note
the wall-clock; if either check takes more than about 30 s, say so in the commit
message and in `docs/notes.md`, because `.config/nextest.toml` states as fact
that nothing exceeds 0.75 s and justifies its slow mark against that.

- [ ] **Step 3: Confirm they skip rather than fail without the corpus**

Run: `cargo test -p taiyaku-serve corpus::`
Expected: PASS, with both new checks skipping. Since O30 the skip is not silent
— check the output says so.

- [ ] **Step 4: Commit**

```bash
git add crates/taiyaku-serve/src/corpus.rs
git commit -m "Two standing checks over real material: a 1.49 GB single member byte for byte, and a DEFLATE multi-member edition rebuilt"
```

---

### Task 11: the gate, the measurements, and the document

**Files:**
- Modify: `docs/notes.md`
- Modify: `docs/status.md`

- [ ] **Step 1: Run the full gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: the script's own `all green` line. **Quote it in the commit
message.** Do not pipe it through `tail` and do not append `; echo` — both mask
the exit status.

- [ ] **Step 2: Run the corpus job separately**

```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

Expected: every standing check passes, now including two more.

- [ ] **Step 3: Write the measurements into `docs/notes.md`**

Four figures this branch asserts, each with the command that produced it and
the date (2026-08-23):

1. **The entry compression split.** 11,431 of 11,681 entries across 140
   archives are `DEFLATE`; 250 are `STORE`; 4 of 140 archives are dominantly
   stored. This is why a generated CBZ inflates 97.9% of what it stores, and
   why §13.1's "generation is a copy" was corrected.
2. **How often passthrough fires.** 31 of the 47 directories holding
   page-bearing archives hold exactly one (~66%).
3. **What zip64 is not exercised by.** Largest single archive 1.49 GB
   (`corpus/incoming/nausicaa/en-deluxe-v01.cbz`); largest summed group the
   same; 0 of 47 groups over 4 GiB.
4. **Whatever Task 10 measured for wall-clock**, if either check ran long.

Record the reproduction for each. Nothing here goes in `status.md` —
`status.md`'s preamble promises every claim there traces to a measurement
recorded in notes.

**One comment in the code is waiting on this step.** `cbz.rs`'s module doc says
`docs/notes.md` gains the DEFLATE/STORE figure when this task writes it. Once
figure 1 is in, that sentence is true — check it reads correctly rather than
leaving a forward reference pointing at nothing.

- [ ] **Step 4: The `status.md` sweep, on the way in**

```bash
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

At the spec commit these read 65 rows, 34 closed, and "the thirty-one open
items". Confirm they still agree with each other before changing anything; a
disagreement now is someone else's drift and is worth fixing separately.

- [ ] **Step 5: Close O54, open one, in five parts each**

**Closing O54** is not a tick. All five:

1. the table row's checkbox → `[x]`
2. the row's annotation
3. the prose bullet, rewritten to say `can_download` now gates the two download
   routes and `can_stream` the page routes and the PSE stream, that a thumbnail
   deliberately follows browse, and that nothing can set either flag to 0
   outside a fixture until C4
4. the test count, which appears **twice** — in "Six crates, N tests" at line
   120 and again two lines later in "Nine of the N are standing checks". The
   second number moves by two, since Task 10 added two standing checks
5. the "N are now closed — leaving M" sentence, and the "What remains … M open
   items" line near the bottom, whose count word **wraps across a line
   break** — grep the sentence opener and read the next line

**Opening the deferred-Range item** is the same shape inverted: the total count
word at the head of the section ("Sixty-five, each found by…") goes to
sixty-six, the "leaving N" sentence moves, a table row appears, a prose bullet
appears, and the "What remains" line moves. Its site is
`download.rs::generated` and `cbz::clipped`; its evidence is that a resume
reads and discards the prefix rather than seeking to it, that
`Plan::payload_span` is already the map a true seek needs, and that its trigger
is a **measurement** — a fresh `taiyaku serve --log-requests` session against
Panels, bound to the LAN, showing `Range` on an acquisition fetch. The existing
`panels*.jsonl` logs predate request-header capture and report `(not recorded)`
for headers, so their silence is an instrument's blind spot rather than data.

- [ ] **Step 6: Then grep the bare O-number and read every hit**

```bash
grep -rn 'O54' docs/ web/src/ crates/
```

The five parts are necessary, not sufficient: prose *outside* the table also
describes open items, in wording no sweep can predict. `auth.rs:257`'s doc
comment on `Principal` says in the present tense that both flags "govern
nothing" and that O54 tracks all three fields — that is now false and is
rewritten. `browse.rs`'s and the auth spec's mentions are historical narrative
and are marked (`*O54 has since closed — see its entry above.*`) rather than
rewritten.

Also update the crate table's `taiyaku-serve` paragraph and the "Seven
commands" synopsis block if either describes the route surface, and
`crates/taiyaku-serve/Cargo.toml`'s `description` field, which currently says
this crate hands a reader "the sidecar, pages, and progress" and no longer
lists everything it serves.

- [ ] **Step 7: Sweep again, on the way out**

Re-run Step 4's four greps. Rows 66, closed 35, "leaving thirty-one" — check
the arithmetic against the document rather than against this plan, since
another branch may have landed in between.

- [ ] **Step 8: Re-run the gate and commit**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add docs/notes.md docs/status.md crates/taiyaku-serve/src/auth.rs crates/taiyaku-serve/Cargo.toml
git commit -m "The measurements this branch asserts, and O54 closed with the item its deferral opened"
```

- [ ] **Step 9: Open the PR**

Through the **gitea** MCP, not `gh` — `gh` on this machine is authenticated
against the wrong host. The PR body **is** the squash commit message, so write
it as one: what shipped, what was measured, what was corrected in §13.1, what
O54's closure means, and what the new item's trigger is. **Never hard-wrap a PR
body.**

---

## Self-Review

**Spec coverage.** §2 routes → Tasks 7, 8; §2.1 refusals → Task 9; §2.2
`can_stream`'s line → Task 9; §3 the two shapes → Task 7; §4 the plan pass →
Tasks 1, 2, 3; §5 the stream pass and ranges → Tasks 4, 7, 8; §6 ComicInfo →
Task 5; §7 the filename → Task 6; §10 testing → in every task, plus Task 10's
corpus checks; §12 bookkeeping → Task 11. **§8 (the tile control) and §9 (the
OPDS acquisition link) are deliberately not here** — they are the second plan,
`2026-08-23-library-download-clients.md`, written when this one lands. §11's
"deliberately not done" list needs no task by construction.

**Two places this plan knowingly hands work to the implementer rather than
writing it out.** Task 10's corpus helpers, because `corpus.rs` has an
established idiom for reaching real material and a second one would be the
drift O30 closed; and Task 9's route-count constant, because the number is in a
file this plan should not guess at. Both name exactly what to read first. Every
other step carries its code.

**Type consistency.** `PlannedEntry`/`PageEntry` are two types on purpose —
`taiyaku_read::PageEntry` describes a source entry, `cbz::PlannedEntry`
describes an output entry, and the renumbering in `prepare` is the map between
them. `clipped` takes `(bytes, region_start, from, upto)` in that order at
every call site. `refuse_if_cannot_download`/`refuse_if_cannot_stream` both
take `&Extensions` and return `Option<Response>`, matching `refuse_if_hidden`.

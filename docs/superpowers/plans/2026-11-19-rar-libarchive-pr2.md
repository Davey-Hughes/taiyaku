# RAR through libarchive, PR 2: libraries read RAR — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A build with the `rar-libarchive` feature (the image) counts, derives, fingerprints, serves and downloads RAR members as it does zips, and refuses at the count, by name and cause, any RAR it could not read whole.

**Architecture:** `Format::PAGED` gains `Rar` under `#[cfg(rar)]`, which switches `is_readable`, the derive and the fingerprint gate in one edit. Before that switch, the libarchive backend's own header walk (`rar::libarchive::headers`) runs at listing as well as at reads, over the one open file, so that what libarchive would fail on later (a volume of a set, RAR 2.0 compression, an encrypted entry, a duplicate name, a reference with no earlier target) is refused before anything is counted. The reader signature gains a RAR arm naming libarchive's build-time version, so a scan counts every RAR that a build without the feature left uncounted, and does so again after a libarchive upgrade.

**Tech Stack:** Rust (taiyaku-read, taiyaku-library, taiyaku-serve, taiyaku-cli); `compress-tools` from the owner's fork, pinned at d823828, **unchanged**; the system libarchive, Debian trixie's 3.7.4 in the image and CI, 3.8.11 on the dev machine; `pkg-config` at build time; TypeScript for the licence page.

**Spec:** `docs/superpowers/specs/2026-11-17-rar-libarchive-design.md` ("Readability", "Re-reading when the reader changes", "Decisions" → "Two PRs", "Checks on real material (PR 2)"), and PR 1's plan `docs/superpowers/plans/2026-11-17-rar-libarchive-pr1.md`, "What PR 2 inherits" (line 478). Pre-flight evidence: `.superpowers/sdd/rar-pr2-evidence/` `preflight-sites.md`, `preflight-rows.md`, `preflight-fork.md`; the rulings: `ledger-pr2.md`.

## Owner rulings (2026-11-19, before this plan; do not re-ask)

- **P2-R1 (O384):** fix it here: `member_page` lists and reads a RAR through one open file.
- **P2-R2 (O385):** a RAR holding two entries of one name is refused at listing, as a RAR4 whose next entry repeats a name already is.
- **P2-R3 (RAR 2.0, O386):** refused at the count from the header's unpack version and method. Nothing is decoded at scan time.
- **P2-R4 (O381):** the proper fix. The fork cannot supply it: libarchive never gives a `rar -oi` reference's target, and marks a RAR5 entry encrypted only from 3.7.7 (`preflight-fork.md` §1–2). So taiyaku's own walk does it: an empty file reads as empty, a reference or hard link reads its target, and an encrypted entry is refused.
- **P2-R5 (reader signature):** names libarchive's **build-time** version, read by `pkg-config --modversion libarchive` in taiyaku-read's `build.rs`. No fork change.
- **P2-R6 (volumes):** a volume of a multi-volume set, RAR4 or RAR5, is refused at the count.
- Standing: encrypted archives are out of scope; no push to the fork; BLAKE2-only entries stay unchecked (R22, O383); O386 stays open (option A); taiyaku supports RAR for its users, and the owner's own library aims to hold none.

## Global Constraints

- RAR-specific code stays inside taiyaku-read behind `#[cfg(rar)]`. taiyaku-library, taiyaku-serve and taiyaku-cli carry no cfg of their own (none does today): their code and tests ask `Format::PAGED`, `Format::is_readable` or `reader_signature` at run time, and a test that differs by build branches on `Format::Rar.is_readable()`.
- A build without the feature behaves as main does. It refuses RAR by name with the message at `pages.rs:390-393`, and `reader_signature("rar")` is `None`. CI's required `default-features` job proves this. Every test that depends on the build passes in both builds.
- `unsafe_code = "forbid"` stays. No fork commit, and Cargo.lock's `compress-tools` pin does not move.
- Never a wrong page. A RAR is refused with `"{path}: not a readable RAR archive: {cause}"`, the error shape the `rar::Backend` doc gives. Where the refusal is a rule above the interface, it sits in `pages.rs` ("the page rules belong above this interface", `rar.rs:1-2`).
- Tests pass on libarchive 3.8.11 (here) and 3.7.4 (CI). A fixture that behaves differently on the two says so in its manifest or test.
- Comments say why (CLAUDE.md). History and measurements go in `docs/notes.md`.
- Fixture pages are generated images, and no published page enters the repository. RAR 2.0 fixtures are made with RAR 2.80 as `.superpowers/sdd/rar2-upstream-evidence/repro-report.md` line 14 does: an i386 throwaway container, by absolute path, never installed on the host. `make.sh` records the command.
- Throttles: `--test-threads=8`; `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate. Run a mutation sweep only under `flock /run/user/1000/taiyaku-mutants.lock`, after `pgrep -x cargo-mutants` comes back empty.
- Real material: the NAS is read only (`/mnt/daveynet/nfs/media 1`), and the server serves a scratch **copy** made with `cp`, never `ln`, never the NAS itself.

## Review Focus

1. **A RAR counted whose pages then fail.** Any listing rule that admits what a read refuses. Task 1 adds a test over every fixture: a manifest that is not `refused` reads every page whole through `for_each_member_page`. Task 7 runs the same check over the real archives.
2. **Rollback.** A store scanned with the feature, then without it, then with it again: works stay visible, pages are refused with the no-feature message, saved places are unchanged, and the no-feature scans do not rewrite RAR rows. One sniff per RAR per scan is the accepted cost (`preflight-sites.md` §3). Task 4 adds a scan test for the last point; Task 7 tests the rest on real material.
3. **A RAR named `.cbz`** (17 of the census's 21) is served, downloaded and advertised with RAR's type and the `.cbr` extension, and a zip named `.cbr` the other way round. Task 5.
4. **Replaced mid-read (O384), and a duplicate name (O385).** Never a wrong page, and every call agrees. Tasks 1 and 2.
5. **An empty entry of a solid RAR5.** libarchive fails to read it on both versions (`preflight-fork.md` §7), so it must read as empty without asking libarchive. Task 2.

## Decisions this plan takes (the owner can overturn any at approval)

- **D1. The walk runs at listing too.** `Libarchive::entries` walks the block headers over the same `File` that libarchive lists from. P2-R3 to R6 need this. It also narrows O351's first gap: the owner ruled on 2026-11-17 not to build it "at the time".
- **D2. Listing refusals,** each `not a readable RAR archive: <cause>`:
  - a volume of a set (RAR4 main-header flag 0x0001; RAR5 main-header archive flag 0x0001) [P2-R6];
  - a regular-file entry compressed by a method libarchive's RAR4 reader does not decode [P2-R3]. The set comes from reading 3.7.4's `archive_read_support_format_rar.c`, and a test pins it. A stored entry (method 0x30) is read whatever its version: FF v1 247 and the Pokemon RAR4 each hold a v20 `-m0` entry (`bump/review-3.md`:124);
  - an encrypted entry (RAR4 file flag 0x04; the RAR5 encryption extra record) [P2-R4, standing ruling];
  - a reference or hard link whose target is not an **earlier** readable entry of exactly that name [P2-R4, `preflight-fork.md` §5];
  - two readable entries of one name [P2-R2]. This rule goes in `pages.rs`, so it holds for every backend.
- **D3. Reads.** An empty regular file reads as `Ok(vec![])` without asking libarchive. A reference or hard link reads its target's bytes, which must meet the target's CRC32 and the reference's declared size [P2-R4].
- **D4. One open.** `member_page` lists, picks the page and reads it by **archive position** through one `File` [P2-R1]. `read` by name stays for `archive_page` and `member_entry`, where D2 makes "the first entry of that name" the only one.
- **D5. Signature.** `reader_signature("rar")` in a feature build is `rar:libarchive-<build-time version>:<IMAGE_EXTENSIONS joined by ",">`. Changing any listing rule changes the string, as the doc at `pages.rs:26-33` already requires of every format [P2-R5].
- **D6. A download's type and extension follow the stored format token** (`scan_entry.format`, sniffed at scan), not the file name, sniffing the file only where the store has none. The OPDS test that pins type by extension (`opds/tests.rs:2691-2715`) changes with it. No closed ruling covers it (`preflight-rows.md` §1.10).
- **D7. The generated download takes a RAR page's CRC32 and size from the walk.** It decodes only BLAKE2-only entries, which carry no CRC32. A damaged page then fails mid-stream, as a damaged zip page does.
- **D8. The fingerprint pass's renumber rule** binds the signature of every paged file format, not only zip's.
- **D9. A refused count's cause is logged.** `scan::pages_of` drops it today (`.ok()`, `scan.rs:3619`), for zips too. It is logged at `warn` with the path. The store still keeps none: O131 stays open. This is what "with the reason given" (P2-R3) means in this PR.
- **D10. The rollback is proved on a store** that a feature scan built from the scratch library of real archives, using a copy of that store. The NAS's live store is not used.
- **D11. Rows.** New: **O391**, RAR reading in libraries, opened and closed here (re-check that the number is free before pushing). Close O381, O384 and O385. Narrow O351 (the listing walks) and O386 (refused at the count). O380, O382 and O383 stay open. O90's record gets the pointer the design asks for.

## Evidence layout

`$EV` is `/home/davey/projects/taiyaku/.superpowers/sdd/rar-pr2-evidence/` in the main checkout (gitignored). For each task: `T<n>-*.log` with `.exit` files, and reviews in `T<n>-review*.md`. Real material goes in `$EV/real/`. The worktree is `.claude/worktrees/rar-pr2` (branch `rar-pr2`), with its own `CARGO_TARGET_DIR`.

---

### Task 1: The walk at listing, and what it refuses

**Dispatched in two halves**, so each stays within CLAUDE.md's ~150-call size. **1a:** the walk at listing, volumes, encryption and duplicate names. **1b:** the RAR 2.0 rule (reading 3.7.4's `archive_read_support_format_rar.c`, the RAR 2.80 fixtures) and the dangling reference. Each half runs Steps 1–6 for its own refusals, and 1b starts from 1a's commit.

**Files:**
- Modify: `crates/taiyaku-read/src/rar/libarchive/headers.rs`: `Header` (:34), `Walk` (:45), `rar5_fields` MAIN arm (:228-231), `rar5_file` (:251), `rar5_extra_leaves_bytes_own` (:290, which already sees ENCRYPTION and REDIRECTION), `rar4_fields` MAIN arm (:335), `rar4_file` (:364)
- Modify: `crates/taiyaku-read/src/rar/libarchive.rs`: `entries` (:23-42), `open` (:89-93)
- Modify: `crates/taiyaku-read/src/rar.rs`: `Entry` (:14-28) and its docs
- Modify: `crates/taiyaku-read/src/pages.rs`: `rar_entries` (:1101-1110), for the duplicate-name rule
- Fixtures: `crates/taiyaku-read/fixtures/rar/make.sh` and manifests. These flip to `refused`: `rar4-split.part1`, `rar4-split.part2`, `rar5-split-last.part2`, `rar4-encrypted-entry`, `rar5-encrypted-entry`. New: `rar4-v20.rar` (RAR 2.80 `-m3`, two generated PNGs, refused), `rar4-v20-stored.rar` (RAR 2.80 `-m0`, read), `rar5-dangling-reference.rar` (`rar a -oi`, then `rar d` on the target, refused).
- Test: `crates/taiyaku-read/src/pages/rar_tests.rs`, `crates/taiyaku-read/src/rar/tests.rs`

**Interfaces:**
- Produces: `Entry` gains `pub(crate) target: Option<usize>`, the archive position of the earlier entry whose bytes a reference or hard link reads. `size` becomes the walk's declared size: a reference's is its target's. `readable` keeps its meaning. `Walk` carries what Task 2 needs per entry (CRC32, kind, target name); the field names are the implementer's.

- [ ] **Step 1: Write the failing tests.** Each refused fixture's error contains its cause: "one volume of a multi-volume set", "RAR 2.0 compression" (or the exact wording chosen), "encrypted", "two entries are named `p01.png`" (built at run time with `rename_rar5_entry` and `rename_rar4_entry`, including a non-adjacent RAR4 duplicate), and "its target … is not an earlier entry". The controls `rar4-v20-stored.rar` and `rar5-file-reference.rar` still list. Add the Review Focus 1 test: for every fixture in `test_support::rar_fixtures()` whose manifest is not `refused`, `for_each_member_page` hands over every page the listing names, whole.
- [ ] **Step 2: Run them and watch each fail** for the reason named: `cargo test -p taiyaku-read --all-features -- --test-threads=8 rar` into `$EV/T1-red.log`.
- [ ] **Step 3: Implement D1 and D2.** `entries` opens once and runs the walk over that `File` before listing through `iterate(file.try_clone()?)`. The walk's entries pair with libarchive's the way `read` already pairs them for the CRC check.
- [ ] **Step 4: Run taiyaku-read's suite under all features and under default features** (`$EV/T1-green-all.log`, `$EV/T1-green-default.log`). Then run the rar suite in the `rust:1-trixie` image against 3.7.4 (`$EV/T1-trixie.log`; the recipe is in `rar-pr2-evidence/trixie-report.md`).
- [ ] **Step 5: Hand-apply three defects and watch the named test fail:** drop the volume check, invert the stored-method exception, and remove the "earlier" condition. Restore each.
- [ ] **Step 6: Commit.**

### Task 2: Empty entries, references, and one open per page (O381, O384)

**Files:**
- Modify: `crates/taiyaku-read/src/rar.rs`: `Backend` (:35-55)
- Modify: `crates/taiyaku-read/src/rar/libarchive.rs`: `read` (:45-62), `visit` (:64-82), `data` (:147-175)
- Modify: `crates/taiyaku-read/src/pages.rs`: `member_page` (:571-603), `rar_pages` (:1115-1123), `rar_visit_keeping` (:1164-1183), `rar_for_each_page` (:1186-1209)
- Fixtures: new `rar5-solid-empty.rar` (solid, one empty page between two pages) and `rar5-hard-link.rar` (`rar a -oh` over a hard link that `make.sh` makes with `ln`)
- Test: `crates/taiyaku-read/src/pages/rar_tests.rs`; R8's test (`rar_tests.rs:250-262`) flips

**Interfaces:**
- Consumes: Task 1's `Entry::target` and the walk's per-entry CRC32.
- Produces: `Backend::read_listed(&self, path: &Path, pick: &mut dyn FnMut(&[Entry]) -> Result<usize, String>) -> Result<Vec<u8>, String>`, which lists and reads through one `File`. `pick` returns an archive position. The listing-to-pages step that `rar_pages` does today becomes a pure function over `&[Entry]`, so that `member_page`'s `pick` can use it.

- [ ] **Step 1: Write the failing tests.**
  - O384: inside `pick`, rename another fixture over the path, and assert that the bytes are the original's page at that index.
  - D3: the empty page of `rar5-empty`, `rar4-empty` and `rar5-solid-empty` reads as `[]` through `member_page`, `for_each_member_page`, `member_page_entries` and `read_edition`.
  - An empty `ComicInfo.xml` gives `member_entry` `Ok(Some(vec![]))`.
  - The references in `rar5-file-reference` and `rar5-hard-link` read their target's bytes through every call, including a `for_each_member_page` that wants the reference and not the target.
- [ ] **Step 2: Run them and watch them fail** (`$EV/T2-red.log`).
- [ ] **Step 3: Implement `read_listed` and D3.** `visit` keeps the bytes of a target that a later wanted reference needs. The `member_page` RAR arm calls `read_listed`.
- [ ] **Step 4: Run both builds, and the trixie run, as in Task 1 Step 4.**
- [ ] **Step 5: Hand-apply two defects and watch the named test fail:** `member_page` back to list-then-read-by-name, and an empty entry read through libarchive.
- [ ] **Step 6: Commit.**

### Task 3: The reader signature names RAR and libarchive's build-time version

**Files:**
- Modify: `crates/taiyaku-read/build.rs`. With the feature on, read libarchive's version through pkg-config and export it as `cargo::rustc-env=TAIYAKU_LIBARCHIVE_VERSION`. If pkg-config gives no answer, fail the build with a message naming `libarchive-dev`. Use the `pkg-config` crate already in Cargo.lock (through compress-tools) as a build-dependency, and emit no link lines of its own.
- Modify: `crates/taiyaku-read/Cargo.toml` (`[build-dependencies]`); `crates/taiyaku-read/src/pages.rs` `reader_signature` (:26-38) and its doc; `crates/taiyaku-read/src/rar.rs:39` (the `expect(dead_code)` comes off `name`)
- Test: `crates/taiyaku-read/src/pages/tests.rs:1342-1365`, split by cfg

**Interfaces:**
- Produces: `reader_signature("rar")` → `Some("rar:libarchive-<version>:<extensions>")` with the feature, and `None` without it.

- [ ] **Step 1: Write the failing test.** With `rar`: the signature equals `format!("rar:libarchive-{}:{}", env!("TAIYAKU_LIBARCHIVE_VERSION"), IMAGE_EXTENSIONS.join(","))`, and the version is the one `pkg-config --modversion libarchive` prints (3.8.11 here; record it in `$EV/T3-version.log`). Without the feature: `None`.
- [ ] **Step 2: Watch it fail. Then implement, and watch it pass in both builds and in the trixie container, where the version reads 3.7.4.**
- [ ] **Step 3: Confirm the Dockerfile's server stage and every CI container that builds with `--all-features` has pkg-config and `libarchive-dev`** (grep, and quote the lines in `$EV/T3-build-env.md`).
- [ ] **Step 4: Commit.**

### Task 4: Libraries count, derive and fingerprint RAR

**Files:**
- Modify: `crates/taiyaku-read/src/format.rs`: `PAGED` (:52) gains `#[cfg(rar)] Self::Rar` (as `rar::BACKENDS` does, `rar.rs:58-60`). Update the docs at :8-18, :49-51 and :97-109: keep the O90 record and add the pointer to the design and P2-R1 to R6.
- Modify: `crates/taiyaku-library/src/scan.rs`. `pages_of` (:3611-3640) counts every paged file format (`Format::from_token` against `PAGED`, less `Dir`), and its doc (:3594-3610) changes to match. Add D9's `warn`.
- Modify: `crates/taiyaku-library/src/fingerprints.rs`: `numbers_other_pages`, `zip_signature` and `due` (:95-109, :165-185), per D8
- Modify: docs at `derive.rs:77-80` and `libraries.rs:296-300`
- Test: `crates/taiyaku-read/src/format/tests.rs` (:178-199, :206-222); `crates/taiyaku-library/src/derive/tests.rs` (:3386-3447, branching on `Format::Rar.is_readable()`: with the feature a bare RAR header lands in `uncounted`, without it in `by_format["rar"]`); `crates/taiyaku-library/src/scan/tests.rs` (beside :12554); `crates/taiyaku-library/src/fingerprints/tests.rs` (:83-148, :719, :837)

- [ ] **Step 1: Write the failing tests** (feature build):
  - A RAR fixture in a scanned directory is a member with the manifest's page count (template `derive/tests.rs:3452`; fixtures from `taiyaku_read::test_support::rar_fixtures()`).
  - A `refused` fixture is uncounted, and the scan's log names its cause.
  - A RAR row whose `counted_by` is NULL is counted again without hashing (template `scan/tests.rs:12554`).
  - The rollback order, feature then no-feature then feature, keeps the row, its sha256 and its count, and the no-feature scan does not rewrite the row.
  - A RAR row whose page count moved under its fingerprint row is due again (D8).
- [ ] **Step 2: Watch them fail** (`$EV/T4-red.log`).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run taiyaku-read, taiyaku-library, taiyaku-serve and taiyaku-cli under both builds** (`$EV/T4-green-*.log`). These are whole crates, not named test files.
- [ ] **Step 5: Hand-apply two defects and watch the named test fail:** D8 restricted to zip again, and `pages_of` without the RAR arm.
- [ ] **Step 6: Commit.**

Merge-order risk: `content-volume-joins` rewrites `derive.rs` and touches `derive/tests.rs` and `fingerprint_pass` (`preflight-rows.md` §1). Whichever of the two merges second rebases.

### Task 5: The server serves and downloads RAR members

**Files:**
- Modify: `crates/taiyaku-serve/src/download.rs`: `passthrough_extension` (:185-195), `advertised_type` (:215-220), `type_for` (:230-238), `prepare` (:638-664, and the stale comment at :654-655), `prepare_member` (:1184-1231). Per D6.
- Modify: `crates/taiyaku-serve/src/cbz.rs:21-23` (stale comment); `crates/taiyaku-serve/src/opds.rs:703`
- Modify: `crates/taiyaku-read/src/pages.rs`, the `member_page_entries` RAR arm (:846-850, `rar_page_entry` :1152-1161), per D7
- Test: `crates/taiyaku-serve/src/opds/tests.rs` (:2691-2715 changes), plus route tests beside the existing download and page tests, branching on `Format::Rar.is_readable()`

- [ ] **Step 1: Write the failing tests** (feature build, with a scanned RAR fixture):
  - The first and last page answer 200 with the manifest's sha256.
  - `?w=` resizes, and the cover/thumb answers.
  - A generated download of an edition with two RAR members reads back through the `zip` crate with every page's bytes and CRC32 equal to the manifest's.
  - A RAR named `.cbz` passes through as `application/vnd.comicbook-rar` named `.cbr`, and a zip named `.cbr` as `application/vnd.comicbook+zip` named `.cbz`, in the edition download, the member download and the OPDS acquisition link.
  - `member_page_entries` on a fixture whose page data is damaged and whose headers are whole returns `Ok` with the header's CRC32, while `for_each_member_page` refuses it (D7).
- [ ] **Step 2: Watch them fail. Then implement, and run taiyaku-serve and taiyaku-read under both builds.**
- [ ] **Step 3: Hand-apply the defect "type from the extension" and watch it fail. Restore it.**
- [ ] **Step 4: Commit.**

### Task 6: The licence page and README

**Files:** `web/src/licenceTexts.ts` and its test; `README.md` (:468-483, where :481-482 becomes wrong; and the Licence section :911-917).

- [ ] **Step 1:** Add libarchive's BSD-2-Clause notice, taken from libarchive v3.7.4's `COPYING`, marked as part of builds with RAR support (the container image). The README's RAR paragraph says that a library scan in such a build counts and serves RAR, and refuses at the count the kinds D2 lists.
- [ ] **Step 2:** Run the web gate jobs, browser project included (`$EV/T6-web.log`).
- [ ] **Step 3: Commit.**

### Task 7: Checks on real material

No code. Everything goes in `$EV/real/`.

- [ ] **Step 1: The census.** Write `$EV/real/census-pr2.txt`:
  - the census's 20 RARs still in the library (`rar-libarchive-evidence/census.txt` less FF v1 360);
  - `Magazines/Nintendo Power/2000/Supplements/Pokemon Ep.14 - Electric Shock Showdown (March 2000).cbz`;
  - FF v1 360's original from `needs_processing/taiyaku-originals/Comics/Issues/Fantastic Four/Fantastic Four v1 (1961-1996)/`, as the expected RAR 2.0 refusal.

  Run `unrar lt` on the Pokemon file and log the result.
- [ ] **Step 2: Copy the 22 files with `cp`** into `/home/davey/temp/rar-pr2-lib/`, keeping relative paths, and check each copy's sha256 against its source.
- [ ] **Step 3: The probe.** Repoint `trixie-probe`'s `Cargo.toml` at the worktree and run `t8real` over `census-pr2.txt` on 3.7.4 (container) and on 3.8.11. Every page of the 21 reads whole, and FF v1 360 is refused **at listing** with D2's cause.
- [ ] **Step 4: Scan the scratch library with a feature release build.** Each of the 21 is counted with the number of pages `bsdtar -tf` lists after the same filter. FF v1 360 is uncounted, and the log names the cause.
- [ ] **Step 5: Serve it** (the recipe is in memory `taiyaku-ui-work-prototype-on-a-live-server`: 127.0.0.1:4399, `local`/`pw`). For each archive, the first and last page equal `unrar p`'s bytes. Also check a cover, a resized page, a passthrough download (RAR type and `.cbr`), and a generated download of an edition with several RAR members (`unzip -t` clean, pages equal). Set a saved place in three works.
- [ ] **Step 6: Rollback, on a copy of that store.** Scan it with a build without the feature: the works are visible, the pages are refused with the no-feature message, and the saved places are unchanged. Scan it with the feature again: the works read, and the places are unchanged.
- [ ] **Step 7: Report** in `$EV/real/report.md`. Keep the scratch library and builds until the owner says otherwise. Stop every server by PID and re-check the port.

### Task 8: Gate, mutation sweep, image and docs

- [ ] **Step 1: Docs** (D11):
  - `docs/status.md` rows: O391, plus the closes and narrows;
  - a `docs/notes.md` section with the rulings, D1–D10 and the real-material results;
  - the design's PR 2 notes;
  - `docs/ci.md`, if build.rs's pkg-config needs a line.
- [ ] **Step 2: Build the image locally from the branch.** Check that it starts healthy and answers `/` with 200, then remove it by id (recipe in `bump/report.md`).
- [ ] **Step 3: Run the mutation sweep over the branch diff** (`scripts/mutants.sh --since main`) under the flock. Check the unviable count against what changed, and re-key the baseline last.
- [ ] **Step 4: Run the full gate** (`scripts/check-all.sh`) into `$EV/gate-1.log` with `.exit` and `.started` files, after reading `/proc/loadavg`. Quote the `all green` line.
- [ ] **Step 5: Draft the PR body** at `$EV/pr-body.md`, not hard-wrapped. It must say that merging deploys to the NAS, and that the library's next scan counts its 21 RARs.

### Finish

1. A whole-branch review, with passes until severity falls. Review the fixes too, and grep for a fixed finding's shape, not only its function.
2. Claim-check the rows, the notes and the PR body. Scope each later pass to the correction commit and the PR body.
3. Before the final gate, check main for merges, warn the owner, and rebase if needed. Re-check that O391 is free.
4. The owner pushes, or says to push, and opens the PR on the forge.

---

## Self-review

- **Spec coverage.** Readability: Task 4. The signature: Task 3. The licence: Task 6. The checks on real material: Task 7. Each "What PR 2 inherits" item: the sidecar is covered by Task 5's route tests; O381 and O383 by D3 and R22; O386 by D2; the lone volume by D2 and P2-R6; the switch by Task 4; the signature by Task 3; the licence by Task 6; real material by Task 7; the row by D11; `default-features` is already required.
- **Types.** `Entry::target` (Task 1) is used by Task 2. `read_listed` (Task 2) is used by `member_page` only. `TAIYAKU_LIBARCHIVE_VERSION` comes from Task 3 alone.
- **Proportion.** No function bodies. Every step names its test, its file:line, and the check that proves it.

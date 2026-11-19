# RAR through libarchive, PR 1: the interface, the backend and the build — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `taiyaku-read` reads RAR4 and RAR5 pages through a private, swappable interface with a libarchive backend behind the default-off feature `rar-libarchive`, every build path (crates, CI, the gate, mutants, the image) compiles and tests it, and the library and the server still refuse RAR (except a sidecar's RAR members in a feature build: Decision 8).

**Architecture:** Task 1 makes the shared fixtures and checks `compress-tools` against four pass criteria; any failure stops the plan for the owner. `taiyaku_read::rar` is a `Backend` trait with one implementation, `rar/libarchive.rs`, selected by a cfg alias that a dependency-free `build.rs` sets. `pages.rs` dispatches on the sniffed format and applies its page rules (filter, natural sort, unreadable entries skipped, crc32 from the bytes) above the interface, once, for every backend. One suite holds every compiled backend to the fixtures' manifests, again under `LC_ALL=C`. Nothing in `taiyaku-library` or `taiyaku-serve` changes but their manifests, so `Format::is_readable`, the derive and the scan keep RAR out.

**Tech Stack:** Rust (`compress-tools` 0.16.x over the system libarchive; `zip` 9.1.3, `crc32fast`, `sha2` already in the crate); RARLAB `rar` 7.23 (installed) and 6.24 (fetched for RAR4) and ImageMagick 7 for the fixtures only; Forgejo Actions on `rust:1-bookworm` (libarchive 3.6.2); Docker (`debian:bookworm-slim` runtime).

**Spec:** `docs/superpowers/specs/2026-11-17-rar-libarchive-design.md` (owner-approved 2026-11-17, commit 81d09e41): The feature, The interface, Swapping safely, CI and the local gate, and the Decisions ("compress-tools is checked before anything is built on it", "Two PRs": PR 1 only).

**O-numbers:** main's table ends at O340. Claimed elsewhere: O336 and O341 (paused-frozen), O337 (resize-polish). The jxl-pages plan is written alongside this one, so O342–O344 are left to it. **This branch claims O380–O382:** O380 is the solid-archive row (Task 7); O381–O382 are for anything the work or its review turns up. Task 7 Step 1 re-checks before writing, and an unused number is released in the What's next memory.

## Owner rulings (2026-11-17, after this plan was written)

- **RAR 6.24 for the RAR4 fixtures:** fetch it into the scratchpad as Task 1 Step 2 says. If that fails, or would need installing, stop and ask the owner, who will install it.
- **A build without RAR handles RAR properly everywhere.** CLI commands that take paths reading RAR in a feature build from PR 1 is fine, on this condition: a build without a RAR backend refuses a RAR archive cleanly in every path (library, server, CLI), with a message that names it as a RAR archive and says this build reads no RAR. Never a panic, and never a misleading "not a zip". Scans still count it under `rar`. The `default-features` CI job runs the tests that pin this.

## Amendment: route B (2026-11-17, after Task 1's hard stop)

compress-tools 0.16.1 failed criteria 2 and 3 (and 1 and 4 through them; `docs/notes.md`'s spike section). **This section overrides the task text below wherever they disagree.**

**Owner rulings (2026-11-17, do not re-ask):**
- Route B: patch compress-tools in the owner's fork, https://github.com/Davey-Hughes/compress-tools-rs, branch `taiyaku-rar`, pushed at `d823828a49015f9d9b2541f89b07a56421d4b09c`. Its four commits on upstream master 2c97761: entry names are read under a UTF-8 locale whatever the environment sets; an entry libarchive gives no name is an error, not a crash; libarchive gets a skip callback; a thread is never left using a freed locale. The patched crate passes all four criteria and Review Focus 5 (the notes' subsection "The patched compress-tools"). The spike ran at `cae8b5b`; the owner then had the branch tidied for upstream (prep-pr: comments trimmed, messages rewritten) and force-pushed as `d823828`, whose code is the same.
- Encrypted archives are not a concern for this project. No encrypted-entry flag is added, and the RAR5 gap on libarchive 3.6.2 is accepted. Criterion 2 now reads: an encrypted entry never crashes, hangs or stops the listing.
- No upstream PRs yet.

**A1. The dependency (Task 2).** `taiyaku-read` depends on `compress-tools = "0.16"` (optional, as planned). The workspace `Cargo.toml` gains `[patch.crates-io] compress-tools = { git = "https://github.com/Davey-Hughes/compress-tools-rs", rev = "d823828a49015f9d9b2541f89b07a56421d4b09c" }` under a why comment of at most 4 lines: the release reads names in the environment's locale and crashes on a RAR4 name under C, gives libarchive no skip callback, and can install a freed locale; the patch goes when a release carries the fixes (O382). Check: `cargo tree -p taiyaku-read --all-features -i compress-tools` names that git source and rev, and so does `Cargo.lock`.

**A2. Encryption (Tasks 2 and 3).** Nothing tells an encrypted entry apart.
- `Entry.readable` means "a regular file": directories and links are not. Its doc says encryption is not detected, and points at O381.
- The two encrypted-entry manifests keep `entry p02.png encrypted` (what the archive holds) and gain `page p02.png encrypted` between p01 and p03. Grammar: `page <name> encrypted` is a page listed in reading order whose read fails. `RarFixture.pages` becomes `Vec<(String, Option<String>)>`, where `None` means the read fails.
- Task 2's tests:
  - the kinds test expects `readable == (kind != Dir)`;
  - `an_encrypted_entry_does_not_read` stays;
  - the read test skips `None` pages;
  - on the encrypted fixtures, `visit` hands over p01.png and then returns an `Err` naming p02.png (the iterator stops there).
  - Mutation (a) becomes "`readable` true for a directory": the kinds test fails on `rar5-layout.rar` (`vol/cover.png` is a directory).
- Task 3, on the encrypted fixtures:
  - `member_page_names` lists p02.png, and `member_pages` is 3;
  - `member_page(1)` is an `Err` containing p02.png, while pages 0 and 2 read to their shas;
  - `member_page_sizes` holds 3 sizes;
  - `member_page_entries`, `for_each_member_page` and `read_edition` are each an `Err` containing the path and p02.png.
  - Mutation (2) becomes "the readable filter dropped": `rar5-layout.rar` lists `vol/cover.png`.
- A stored encrypted entry reads back as ciphertext with no error (3.6.2 in both formats; RAR4 on 3.8.11). No fixture holds one. O381 records it; nothing tests it.

**A3. A short entry is never a page (Tasks 2 and 3).** On a truncated archive, the iterator ends an entry with fewer bytes than its header gives (0 of 321 on the half-length copy) before it reports the error. A single-entry read of that entry returns `Ok` with the short bytes and no error at all, because the crate takes libarchive's end of data as success. So:
- `read` and `visit` both compare the bytes received with the entry's declared size. Any difference is `"{path}: {name}: …"`, and the bytes are handed on only after the check.
- A test copy cut inside a page (not only at half length) is read both ways, and both are errors.
- `a_truncated_archive_is_refused_with_its_path` also asserts that `f` is never called with a byte count different from that entry's manifest size.
- Watched: drop the size check from each of `read` and `visit`, and the matching assertion fails.

**A4. CI, the gate and the image (Tasks 4 and 5).** Every build fetches the git dependency from GitHub.
- Task 4 checks that `advisories.yml`'s `--locked --offline` `cargo deny check advisories` still resolves with a git source in `Cargo.lock`. cargo-deny is not on this machine: run the workflow's pinned 0.20.2, with its checksum, in a throwaway container. If it cannot resolve, add a `cargo fetch --locked` before it.
- Task 5's image build proves the Docker stage fetches the dependency.
- `docs/ci.md` gains one sentence: compress-tools comes from the owner's fork, pinned by commit (O382).

**A5. Rows and docs (Task 7).** Open three rows, renumbered together by Step 1 if taken:
- **O380**, the solid-archive row, as planned.
- **O381: a RAR entry taiyaku cannot read is listed as a page.** That covers an encrypted entry, and a RAR5 file reference or hard link (`rar -oi`, refused as "no data", ruling R7). Reading such an entry, or reading the whole archive, fails naming it; a stored encrypted one is served as ciphertext. Owner ruling 2026-11-17: encrypted archives are not a concern. What would decide it: such an archive in a library. The fix: compress-tools surfacing libarchive's `archive_entry_is_data_encrypted` (RAR4 in every version, RAR5 from 3.7.7) and the reference's target (`archive_entry_hardlink`), so taiyaku can skip the entry or read the target's bytes.
- **O382: compress-tools comes from the owner's fork by commit.** What closes it: an upstream release carrying the fixes, after which the `[patch.crates-io]` entry is removed.
- **A fourth row, numbered by Step 1 (the next free number after every lane's claims): two RAR4 gaps.**
  - A RAR4 archive cut inside or exactly between block headers lists as a shorter archive with no error. libarchive reads a short header as the end of the archive, and a RAR4 archive need not carry an end block (A8). The Task 3a review estimated this at about 1 in 30,000–100,000 truncated RAR4 downloads. What would narrow it: walking the block headers to land exactly on the file's end when there is no end block (owner ruling 2026-11-17: not now); it leaves a cut exactly between two blocks, about one byte position per entry, which nothing can see in a RAR4 without an end block.
  - A RAR4 entry name stored in a legacy code page (no Unicode name) reaches taiyaku as its raw bytes: where they are not valid UTF-8, the whole archive is refused as "not a readable RAR archive: invalid utf-8 sequence…" (ruling R11); where they happen to be, the page lists and reads under a wrong name. No page is dropped either way. A zip decodes such names instead. What would narrow it: a RAR4 archive like that in a library, then a fallback decoder (CP437, as zip uses) passed through compress-tools' decode hook; a fallback runs only where UTF-8 decoding fails, so a name whose bytes happen to be valid UTF-8 would still read under a wrong name.
  - `rar.rs`'s `Entry.name` doc points at this row by description; Step 6 checks that the pointer is still true.

Also in Task 7:
- The status.md dependency sentence names the fork.
- The design's Non-goals line ("Encrypted RAR entries: they are skipped, as zip skips them today") and its Approach 1 each get one sentence on the rulings.

**A7. Task 3 is split in two (CLAUDE.md sizing: a task finishes in under ~150 calls).** Each half is one commit, reviewed on its own.
- **3a: the dispatch and the suite.**
  - `open_archive`'s `Archive` enum, `is_listed_page`, `rar_page_names`, and every arm in the table.
  - The `for_each_member_page` branch comment.
  - `pages/rar_tests.rs` with `every_fixture_reads_as_its_manifest_says`, `a_rar_named_cbz_is_read_as_a_rar`, `a_zip_member_and_a_rar_member_read_in_order` and `a_truncated_rar_names_rar_through_every_function`.
  - The A2 expectations for the encrypted fixtures and the file-reference fixture.
  - Mutations 1–5 and 9.
  - `member_entry` on an entry that declares size 0 is an `Err`, like every other RAR read (ruling R8). A test pins it.
- **3b: the locale, the skip, and builds without RAR.**
  - `the_suite_holds_under_the_c_locale` and `reading_rar_pages_skips_what_it_does_not_need` with `bytes_read_of`.
  - The `cfg(not(rar))` refusal, with the owner's "this build reads no RAR" message (pre-flight ruling R1), and the two `pages/tests.rs` RAR tests.
  - The audit of every `ZipArchive::new` outside `open_archive`.
  - The `format.rs` and doc-comment sentences.
  - Mutations 6–8 and 10.
  - The whole-workspace runs in both builds.

**A8. Task 3c: a RAR5 archive must end with its end-of-archive block** (owner ruling 2026-11-17, "a-lite").

Why: libarchive reports a block header it cannot read whole as the end of the archive, in 3.6.2 and 3.8.11 (`rar5.c` `process_base_block`; `rar.c`). So a RAR cut inside a header lists, counts and reads as a shorter archive with no error. That breaks the anti-goal and Review Focus 1. The Task 3a review found it, and measured it: about 1 in 4,000 truncated RAR5 downloads.

- **The check.** The backend's listing (`rar/libarchive.rs`) refuses a RAR5 archive whose last bytes are not a CRC-valid end-of-archive block, with `"{path}: not a readable RAR archive: …"`. Trailing zero bytes after the block are allowed, because rar pads a non-last volume. The check comes from RARLAB's published RAR5 format description, with libarchive's BSD-licensed `rar5.c` as a reference, never from UnRAR code. A comment names the source and says why the check sits in the backend.
- **RAR4 is unchanged.** One of the owner's comics (Fantastic Four v1 360) has no end block, so an end block cannot be required. The RAR4 residual is a row; see A5.
- **Tests:**
  - every RAR5 fixture still reads as its manifest says;
  - for every cut length shorter than the file, on `rar5-solid.rar` and `rar5-compressed.rar`, `member_pages`, `member_page_names` and `read_edition` are each an error containing "RAR" — never `Ok` with fewer pages.
- **Watched:** remove the check, and the cut test fails.
- **Real files:** copies of the owner's 11 RAR5 files are each accepted, recorded in the report. Copy them only with `cp`, onto disk under `$W/target/`, never `/tmp`; check each sha256; delete the copies afterwards.

**A6. Finish.** The PR body says compress-tools comes from the owner's fork and why, names the four fixes, and lists O381 and O382.

**A9. Every read holds a RAR entry's data to its header's CRC32 (owner ruling 2026-11-18, "fix it first"; Tasks 8 and 9).** The second whole-branch review found that libarchive never compares a stored entry's data with its header's checksum, so a damaged stored page read as good, in RAR4 and in RAR5.

- **What is checked.** `rar/libarchive/headers.rs` walks the block headers of the open file libarchive then reads, and gives each entry, by its position in the archive, its size and the CRC32 it must meet. `read` and `visit` refuse an entry whose bytes differ, naming the archive and the entry, and refuse an archive whose walk cannot finish or disagrees with the listing. No dependency is added. A read of one entry of a solid archive does not list on past it, since that would decode every later entry.
- **What is not checked.** An encrypted entry, an entry split across volumes, a file reference, link or directory, an empty file, and a RAR5 entry that declares BLAKE2 and no CRC32. The listing functions do not walk the headers.
- **R22.** The BLAKE2 case stays unchecked in PR 1 and gets a row, O383: all 3,063 files in the owner's 21 RARs carry a CRC32 and none BLAKE2, and RAR5's BLAKE2sp would need a new dependency or hand-written hashing.
- **R23.** libarchive cannot decode three of the owner's good archives: Fantastic Four v1 360 on either version, and one page each of Zelda v01 and v03 on bookworm's 3.6.2, the image's version. `unrar t` passes all three. PR 1 adds no code for them, since each refusal names the archive; O386 records them, and PR 2 must settle them before RAR enters a library.
- **What the check changes.** Volumes joined with `cat` list their pages and every read of them is refused, because the walk ends at the first end block where libarchive reads on; `unrar t` fails them too. A RAR4 cut less than seven bytes into a block's header still lists and counts short, as A5 says, but its reads are now refused (O351). A RAR5 block continued from the previous volume takes no position in the walk, since libarchive does not list one, so a set's last volume reads as it did.

## Global Constraints

- **Where.** Worktree `W=/home/davey/projects/taiyaku/.claude/worktrees/rar-libarchive`, branch `rar-libarchive`. Never `cd` into the main checkout or another worktree; use `env -C` and absolute paths. Every cargo call sets `CARGO_TARGET_DIR=$W/target`. Line numbers below are hints at 81d09e41; locate by symbol.
- **PR 1 only.** At the end, `git diff main --stat -- crates/taiyaku-library/src crates/taiyaku-serve/src crates/taiyaku-cli/src web/` prints nothing. `Format::is_readable`'s answer, the derive's token list, `scan::pages_of`, the reader signature and the licence page are PR 2's.
- **Spec values, verbatim.** Pass criteria: "reading one page of a non-solid archive reads the entry headers and that page's data, not the data of the entries before it" · "it reports an entry as encrypted" · "it gives correct UTF-8 names under `LC_ALL=C`" · "it works with bookworm's 3.6.2". "If any fails, the builder stops and reports, and the owner chooses." Anti-goals: "A RAR counted with fewer pages than it holds, for instance because names outside ASCII were dropped under the container's C locale." · "RAR code that no CI job compiles or tests." · "Any code derived from UnRAR in the build."
- **Names fixed here.** Feature `rar-libarchive` in all four crates, default off. Cfg alias `rar`. Fixtures in `crates/taiyaku-read/fixtures/rar/`. CI job `default-features`. Error texts in Task 2's Interfaces.
- **Evidence.** `EV=/home/davey/projects/taiyaku/.superpowers/sdd/rar-libarchive-evidence` (exists; holds `census.txt`). Write there by Bash (`cat >| $EV/… <<'EOF'`); the Write tool is refused under `.superpowers/`. Never under `target/`. `docs/notes.md` states figures and never cites an evidence path. Task reports are `$EV/T<n>-report.md` and list every watched mutation: the defect, the test, the failure message.
- **Real material is read-only.** `/mnt/daveynet/nfs/media 1` is mounted read-write here: only `cp`, `sha256sum`, `stat` and `unrar l*` touch it, and no scan, server or `chmod` is ever pointed at it. Copies go under the executing session's scratchpad (`SP`, tmpfs: check `df -h /tmp` first). Each delete is its own Bash call with literal absolute paths.
- **Shell.** zsh with noclobber: overwrite with `>|`, start a log with `>|`. `grep` is a ugrep function: use `rg`, or `grep -a` on logs. Never `for x in $var`, never a variable named `path`, quote globs. Kill by PID. Run long commands into a log and read its summary lines.
- **Tests.** `cargo test … -- --test-threads=8` into `$EV/logs/`, then `rg -a 'test result:'` on the log. `taiyaku-read` runs in both builds each time: `--all-features` and default.
- **Watched mutations.** Every new assertion that guards a defect is watched failing: hand-apply the named defect, run, see it fail on that assertion (not a setup panic), restore, check `git diff`. Assert per element; never compare a whole listing with `contains`.
- **Comments say why** (reason, invariant, what breaks); no history, no measurements; a block is at most 8 lines; `node scripts/check-comments.mjs` passes. `crates/taiyaku-read`'s comment share (`scripts/comment-budget.json`) is never raised: move prose to the notes instead. `unsafe_code = "forbid"` stays; `compress-tools` keeps its own.
- **Gate** (Tasks 4, 7, Finish): `env -C $W -u TAIYAKU_CORPUS CARGO_TARGET_DIR=$W/target NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $EV/gate-<n>.log 2>&1; echo "exit $?" >| $EV/gate-<n>.exit`, at `/proc/loadavg` under 24. Never through `tail`. Read the exit file and quote the `all green` line. Before the first gate, `cp -a` (never `ln`) `web/node_modules` and `crates/taiyaku-wasm/pkg` from the main checkout into `$W`; call `./node_modules/.bin/vitest` directly if a web run is needed. CI never runs the vitest browser project; only the gate does.
- **Commits.** One per task unless a step says otherwise, never `--amend`, no push, no attribution. Check the previous task's commit is an ancestor of `HEAD`, and run `git show --stat HEAD` after each.
- **Forge.** Forgejo, not GitHub: the gitea MCP for PRs and CI status, the REST API (token in `~/.config/tea/config.yml`) for branch protection. The PR body is the squash commit message: no hard wraps, title under 255 characters.

## Review Focus

1. **A damaged RAR**, a truncated download named `.cbz`: every public function returns an error naming the file and "RAR", and nothing panics or hangs. Tests in Tasks 2 and 3.
2. **A RAR whose headers are encrypted** (`rar -hp`): it cannot be listed, so `member_pages` is an error naming RAR, never `Ok(0)` or a page list. Fixture in Task 1; tests in Tasks 2 and 3.
3. **A RAR holding no page images** (a `ComicInfo.xml` and a `.txt`): `member_pages` gives the "no page images" error a zip gets. Task 3.
4. **Two requests reading one RAR at once**, as the server's blocking pool does: each gets its own page's bytes. Task 2.
5. **A RAR4 made on Windows**, whose names may carry `\` separators or a legacy code page: the owner's ten RAR4 comics are listed through the binding and compared with `unrar lb`, name by name. Task 1, Step 8.

## Closed and open rulings this plan touches

- **O90 (closed: RAR refused on licence).** The owner reopened it by approving the design. PR 1 leaves the library's refusal in place, so O90's row stays true as written; PR 2 records the reopening in `is_readable`'s doc and its own row.
- **O91 ("Path counts too", `browse::held_by`).** PR 2's rollback relies on it; nothing here touches what it reads.
- **O130 (open, JPEG XL; jxl-pages lane).** `IMAGE_EXTENSIONS` is untouched. A RAR's pages pass the same `is_page_name`, so a `.jxl` page in a RAR is unread exactly as in a zip until O130 closes.
- **O131 (open: the walk keeps no reason for a NULL count).** Untouched; the scan counts no RAR until PR 2.

## Decisions this plan takes (the design left them to the builder, or was silent)

1. The interface is a `Backend` trait with `entries`, `read` and `visit`; `open_archive` returns an `Archive` enum, `Zip` or (under `rar`) `Rar`.
2. The alias `rar` comes from a `build.rs` with no dependencies, whose feature list fixes the backend order.
3. The fixtures sit in `crates/taiyaku-read/fixtures/rar/`, re-admitted by name in `.gitignore` the way the credit-detection fixtures are. No `.cbz` is committed; a test copies a fixture to a `.cbz` name.
4. rar 7.23 cannot write RAR4 (its switch list has no `-ma`). RARLAB's `rarlinux-x64-624.tar.gz` is fetched into the scratchpad for the RAR4 fixtures and never installed.
5. Three fixtures beyond the design's seven: one with encrypted headers (Review Focus 2), one with no pages (Review Focus 3), and a RAR4 encrypted entry, since libarchive's RAR4 and RAR5 readers are separate code and criterion 2 must hold for both.
6. "About the same bytes" is measured: on Zelda v01, the last page's bytes less the first page's are under a tenth of the packed bytes of the entries before the last. On the small fixtures the bound is a half, because the binding hands libarchive 16 KiB per read, so each header costs at least one such read. The fixture form becomes a standing `strace` test.
7. The `default-features` job also runs clippy and the doc-link check without features, and fails if the default dependency graph holds `compress-tools`. The lint job's doc-link check moves to `--all-features`.
8. A feature build's CLI commands that take member paths directly (`align`, `duplicates`, `credits` and the like) read RAR from PR 1 on: they call `taiyaku-read` with no format check. The library's derive and scan do not, because the derive admits by token and the scan counts by token. But a sidecar that names a RAR member, as `align --write` writes one over RAR files, is imported by `library import` and `serve` and its pages served, because `import::sidecar` and `LocalLibrary::page` check no format (found by reading the code while PR 1 was built; no refusal is added, since PR 2 makes this the intended behaviour). README's sentence on this is corrected in Task 7.
9. O380 (no cache for solid archives) opens here, because PR 1 is where solid RAR5s are first read. libarchive refuses a solid RAR4 at its second entry, so one is refused whole (`rar4-solid.rar`).

---

## Evidence layout (`$EV`)

`census.txt` (exists) · `fixtures/` (`rar624.sha256`, `make.log`, `unrar-*.txt`) · `spike/` (`src/`, `runs/<env>-<what>.txt`, `report.md`) · `logs/` · `T<n>-report.md` · `gate-<n>.log`/`.exit` · `mutants/` · `claim-check-<n>.md` · `pr-title.txt` · `pr-body.md`.

---

### Task 1: The fixtures, and the compress-tools spike (a hard stop)

**Files:**
- Create: `crates/taiyaku-read/fixtures/rar/make.sh`. It records how every fixture was made and is never run by a test. It takes the two rar binaries and an output directory as arguments and writes only inside that directory.
- Create: the ten fixtures below and a `<fixture>.manifest` beside each.
- Modify: `.gitignore`. After the credit-detection exception (~:24–28), re-admit `crates/taiyaku-read/fixtures/`, then `…/fixtures/rar/`, then its files, under a comment of at most 4 lines: generated noise images made by `make.sh`, never a published page.
- Modify: `docs/notes.md`, a new section at the end (Step 9).
- Outside the repository: `$SP/rar624/` (the fetched RAR 6.24), `$SP/fixtures-out/`, `$SP/material/` (copies), `$SP/rar-spike/` (the throwaway spike crate, copied to `$EV/spike/src/` at the end).

**The fixtures** (pages are ImageMagick PNGs: noise `magick -seed <n> -size 180x180 xc: +noise Random PNG24:<f>` for about 96 KiB, 148×148 for about 64 KiB, an 8×8 `gradient:` for the small ones):

| Fixture | Made with | Entries, in the order added | Pages, in reading order |
|---|---|---|---|
| `rar5-stored.rar` | rar 7.23 `-m0` | `p01.png`…`p06.png`, noise, ~96 KiB each | the same six |
| `rar5-compressed.rar` | rar 7.23, default method | `p10.png`, `ComicInfo.xml`, `p2.png`, `p1.png` | `p1.png`, `p2.png`, `p10.png` |
| `rar5-solid.rar` | rar 7.23 `-s` | `p1.png`…`p4.png` | the same four |
| `rar5-encrypted-entry.rar` | rar 7.23, three `a` runs, the middle one with `-psecret` | `p01.png`, `p02.png` (encrypted), `p03.png` | `p01.png`, `p03.png` |
| `rar5-headers-encrypted.rar` | rar 7.23 `-hpsecret` | `p01.png` | refused |
| `rar5-layout.rar` | rar 7.23 `-r`, from a tree | `vol/`, `vol/p01.png`, `vol/p02.png`, `vol/.hidden.png`, `vol/cover.png/` (an empty directory), `__MACOSX/vol/._p01.png`, `.DS_Store` | `vol/p01.png`, `vol/p02.png` |
| `rar5-japanese.rar` | rar 7.23 | `第01話/001.png`, `第01話/002.png`, `第02話/001.png`, `表紙.png` | the same, in that order |
| `rar5-no-pages.rar` | rar 7.23 | `ComicInfo.xml`, `notes.txt` | none ("no page images") |
| `rar4.rar` | rar 6.24 `-ma4 -m0` | `p01.png`…`p04.png`, `ページ05.png`, noise, ~64 KiB each | the same five |
| `rar4-encrypted-entry.rar` | rar 6.24 `-ma4`, the middle `a` run with `-psecret` | `p01.png`, `p02.png` (encrypted), `p03.png` | `p01.png`, `p03.png` |

**The manifest grammar** (tab-separated, one fact per line; the reading order is written literally in `make.sh`, never computed by taiyaku's sort):

    format	rar5                       (or rar4)
    entry	<name>	<file|dir|encrypted>	<uncompressed size>   every entry, in archive order
    page	<name>	<sha256>                  every page, in reading order
    other	<name>	<sha256>                  a non-page entry member_entry reads (ComicInfo.xml)
    refused	not a readable RAR archive       the archive cannot be listed

Sizes and sha256 come from the source files (`stat -c %s`, `sha256sum`), independent of any RAR tool.

**Interfaces:** Produces the fixtures, the manifests, `$EV/spike/report.md` (the four verdicts; the `compress-tools` version to pin; the signal by which the binding reports an encrypted entry; whether `rust:1-bookworm` already ships `libarchive-dev` and `pkg-config`; whether the binding exposes libarchive's version), and the notes section.

- [ ] **Step 1: Preflight.** Into `$EV/T1-preflight.txt`: `df -h /tmp` (at least 3 GiB free), `rar`, `unrar`, `bsdtar --version`, `magick -version`, `pkg-config --modversion libarchive`, `docker image ls`, and `git -C $W status --short` (clean).
- [ ] **Step 2: Fetch RAR 6.24** into the new, empty `$SP/rar624/` from `https://www.rarlab.com/rar/rarlinux-x64-624.tar.gz`. Record its sha256 in `$EV/fixtures/rar624.sha256` and extract it there. Run it by absolute path; `rar624/rar/rar` must print "RAR 6.24". If the fetch fails, stop and ask: the RAR4 fixtures have no other source on this machine.
- [ ] **Step 3: Make the fixtures.** Write `make.sh` and run it into `$SP/fixtures-out` (`>| $EV/fixtures/make.log`). Then check each fixture with tools other than libarchive:
  - `unrar lb` lists exactly the manifest's `entry` names, in order (`$EV/fixtures/unrar-<fixture>.txt`);
  - `unrar lt` marks `p02.png` encrypted in both encrypted-entry fixtures;
  - every `page` and `other` line's sha256 equals that of `unrar p -inul [-psecret] <fixture> <name>`.
  - If rar will not store the empty directory or a dotfile, record it in the report and drop that `entry` line; never edit an archive by hand.
- [ ] **Step 4: Commit them into the tree.** Copy the fixtures, the manifests and `make.sh` into `$W/crates/taiyaku-read/fixtures/rar/` and edit `.gitignore`. Then check: `git -C $W check-ignore -v` on every file prints nothing; `git -C $W status --porcelain` lists all 21 files and `.gitignore`; `du -sh` on the directory is under 1.2 MiB.
- [ ] **Step 5: Copy the material** into `$SP/material/`: the Zelda v01 volume and the ten `rar4` files, paths from `$EV/census.txt`. Check each copy's sha256 against its source. Also list `Manga/Raw/RuriDragon/ルリドラゴン v02 DL-Raw.Se.cbr` with `unrar lb` on the NAS. If any entry name is not ASCII, copy that volume too.
- [ ] **Step 6: Write the spike** in `$SP/rar-spike/`: a binary crate depending on the newest `compress-tools` 0.16.x (pinned with `=`; record the version) and `sha2`. Every subcommand opens the file through a counting wrapper (`Read + Seek`, counting bytes returned by `read` and calls to `seek`) and prints those two counts last:
  - `list <file>`: every entry through `ArchiveIterator` with a filter that records the name (bytes as hex and as decoded), the `stat` mode's kind and size, and then refuses the entry, so no data is decoded. It prints each `Err` it meets and whether the listing went on past it. Then the same through `list_archive_entries`.
  - `read <file> <name>`: `uncompress_archive_file` into a `Vec`; prints the length, the sha256 or the error text.
  - `iter <file>`: a full `ArchiveIterator` pass, printing the event sequence (start with name, data total, end, error text).
  - `version`: what the crate exposes of libarchive's version, or "not exposed".
- [ ] **Step 7: Run it in three environments.** Wrap every run in `timeout 60` and write each to `$EV/spike/runs/<env>-<what>.txt`.
  - **A, the host** (libarchive 3.8.11): under the session's locale (record `locale`), and under `env -u LANG -u LANGUAGE -u LC_CTYPE -u LC_MESSAGES LC_ALL=C`.
  - **B, `rust:1-bookworm`** as root in a throwaway container. Mount `$SP/rar-spike` read-write, and the fixtures and the material read-only. First write `dpkg-query -W libarchive-dev pkg-config` to `out/preinstalled.txt`. Then `apt-get install -y --no-install-recommends libarchive-dev pkg-config`, record `dpkg-query -W libarchive13` (3.6.2-*), build the spike with `CARGO_TARGET_DIR=/tmp/t`, copy the binary to `out/` and `chown` `out/` to your uid. Run under `LC_ALL=C` and `LC_ALL=C.UTF-8`.
  - **C, `debian:bookworm-slim`** plus `apt-get install libarchive13`, running B's binary under the image's own environment (no `LANG`) and under `LC_ALL=C`.
- [ ] **Step 8: Judge, into `$EV/spike/report.md`**, each criterion PASS or FAIL with the evidence lines:
  1. **Skip.** In B and C, `read` of Zelda v01's last entry (archive order, from `unrar lb`) less `read` of its first is under a tenth of the packed sizes of the entries before the last (`unrar lt`). The same difference on `rar5-stored.rar` and `rar4.rar` is under half the sizes of the entries between the first and the last. Record both counts and the seeks.
  2. **Encryption.** In A, B and C, for both encrypted-entry fixtures: `list` tells `p02.png` apart as encrypted (a flag, a per-entry error, or any other signal the public API gives, named in the report) and still lists `p03.png`; `read p02.png` fails; `read p03.png` matches the manifest.
  3. **Names under C.** In A under C, B under C, and C both ways: the names `list` prints for `rar5-japanese.rar` and `rar4.rar` are byte-equal to the manifest's `entry` names, and so are those of the RuriDragon copy if Step 5 made one.
  4. **3.6.2.** In B and C every page of every fixture reads with the manifest's sha256. `rar5-headers-encrypted.rar`, and a copy of `rar5-compressed.rar` truncated to half its length, fail with an error, not a panic or a timeout.
  - **Review Focus 5**, judged like a criterion: in C under `LC_ALL=C`, each RAR4 comic's names equal `unrar lb`'s on the host, one by one, separators included.
  - Also record `out/preinstalled.txt` and the `version` output.
- [ ] **Step 9: Notes.** Append `## RAR through libarchive: the compress-tools spike (<date>)` to `docs/notes.md`: the versions (`compress-tools`, libarchive 3.8.11 and 3.6.2, rar 7.23 and 6.24, and why 6.24); each criterion's result with its figures; the RAR4 name comparison; what the fixtures hold and that their pages are generated. No evidence paths.
- [ ] **Step 10: Clean up and commit.** Delete `$SP/material` by its literal path in its own Bash call. Copy the spike's `src/`, `Cargo.toml` and run script into `$EV/spike/src/`. Commit the fixtures, `make.sh`, `.gitignore` and `docs/notes.md`: "RAR fixtures for taiyaku-read, made with RARLAB's rar from generated pages, and what the compress-tools spike found".
- [ ] **Step 11: THE HARD STOP.** If any of the four criteria, or Review Focus 5, is FAIL, stop here. The report goes to the controller, who brings it to the owner with the design's choices: a binding of our own over libarchive (relaxing `unsafe_code` for one module), or something else the owner names. Tasks 2–7 are not dispatched until the owner rules, and a ruling for our own binding means Task 2 is rewritten first. If all pass, the report's pinned version and its encryption signal go into Task 2's brief.

### Task 2: The `rar` interface and the libarchive backend

**Files:**
- Modify: `crates/taiyaku-read/Cargo.toml`. Add an optional `compress-tools = "<Task 1's version>"` (a plain requirement like the crate's others; `Cargo.lock` holds the version the spike checked) and `[features] rar-libarchive = ["dep:compress-tools"]`, with a why comment: default off, because a build without it needs no libarchive and refuses RAR as before.
- Create: `crates/taiyaku-read/build.rs`. It always emits `cargo::rustc-check-cfg=cfg(rar)`, and emits `cargo::rustc-cfg=rar` when any feature in its backend list is set (`CARGO_FEATURE_RAR_LIBARCHIVE`). The list's order is the fixed order: the first compiled backend is the one the library uses.
- Modify: `src/lib.rs` (`#[cfg(rar)] mod rar;`).
- Create: `src/rar.rs`, `src/rar/libarchive.rs`, `src/rar/tests.rs`.
- Modify: `src/test_support.rs` (the fixture loader).
- Modify: `Cargo.lock`.

**Interfaces (produces):**

    // rar.rs, compiled only under cfg(rar)
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct Entry { pub(crate) name: String, pub(crate) size: u64, pub(crate) readable: bool }
      // name: UTF-8, '/'-separated, as the archive names it; size: uncompressed, from the header;
      // readable: a regular file whose data is not encrypted (false for directories, links, encrypted entries)
    pub(crate) type Visitor<'a> = dyn FnMut(&str, &[u8]) -> Result<(), String> + 'a;
    pub(crate) trait Backend: Sync + std::fmt::Debug {
        fn name(&self) -> &'static str;
        fn entries(&self, path: &Path) -> Result<Vec<Entry>, String>;          // archive order
        fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String>;    // the first entry of that name
        fn visit(&self, path: &Path, wanted: &dyn Fn(&str) -> bool, f: &mut Visitor<'_>) -> Result<(), String>;
          // archive order; decodes and hands over only the readable entries that wanted accepts;
          // f's Err stops it at once and is returned unchanged
    }
    pub(crate) static BACKENDS: &[&dyn Backend];       // every compiled backend, in build.rs's order
    pub(crate) fn backend() -> &'static dyn Backend;   // BACKENDS[0], or under cfg(test) the one with_backend set on this thread
    #[cfg(test)] pub(crate) fn with_backend<T>(b: &'static dyn Backend, f: impl FnOnce() -> T) -> T;

    // rar/libarchive.rs
    #[derive(Debug)] pub(crate) struct Libarchive;     // impl Backend; name() == "libarchive"

    // test_support.rs (cfg(any(test, feature = "test-support")))
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum RarEntryKind { File, Dir, Encrypted }
    #[derive(Debug, Clone)] pub struct RarFixtureEntry { pub name: String, pub kind: RarEntryKind, pub size: u64 }
    #[derive(Debug, Clone)] pub struct RarFixture {
        pub file: String, pub path: PathBuf, pub format: String,
        pub entries: Vec<RarFixtureEntry>,      // archive order
        pub pages: Vec<(String, String)>,       // reading order: (name, sha256 hex)
        pub others: Vec<(String, String)>,
        pub refused: Option<String> }
    pub fn rar_fixtures() -> Vec<RarFixture>;   // every manifest under CARGO_MANIFEST_DIR/fixtures/rar, by file name;
                                                // panics naming the file and line on a malformed line
    pub fn sha256_hex(bytes: &[u8]) -> String;

**Error texts, fixed:** a file that will not open or list gives `"{path}: not a readable RAR archive: {cause}"`. An entry that will not read gives `"{path}: {name}: {cause}"`. A name the archive lacks gives `"{path}: {name}: no such entry"`. Each `{path}` is `path.display()`.

- [ ] **Step 1: Write the failing tests** in `rar/tests.rs`. Each runs over every backend in `BACKENDS` × every fixture from `rar_fixtures()`, and its messages name the backend and the fixture.
  - `every_entry_is_listed_in_archive_order_with_its_kind`: per element, `name`, `size` (files) and `readable == (kind == File)`. For a fixture with `refused`, `entries` is `Err` starting with its path and containing the text.
  - `each_page_and_other_entry_reads_to_its_manifest_bytes`: `sha256_hex(read(name))` per `page` and `other` line.
  - `an_encrypted_entry_does_not_read`: `read("p02.png")` on both encrypted-entry fixtures is `Err` containing `p02.png`.
  - `a_visit_decodes_wanted_readable_entries_in_archive_order`: with `wanted` always true, the visited names equal the readable `entry` names in order, each with its manifest sha. With `wanted = |n| n == "p2.png"` on `rar5-compressed.rar`, exactly `["p2.png"]`.
  - `a_visit_stops_at_the_visitors_first_refusal`: on `rar5-stored.rar`, `f` returns `Err("stop")` on its second call, so the result is `Err("stop")` after exactly 2 calls.
  - `a_truncated_archive_is_refused_with_its_path` (Review Focus 1): `rar5-compressed.rar` cut to half its length in a `TempDir`. `entries`, `read("p1.png")` and `visit` each return `Err` starting with the copy's path; `entries`' error contains "not a readable RAR archive".
  - `a_missing_file_names_its_path`, and `a_name_the_archive_lacks_is_no_such_entry`.
  - `six_threads_reading_one_archive_each_get_their_own_page` (Review Focus 4): one thread per page of `rar5-stored.rar`, started together, each sha equal to its page's.
- [ ] **Step 2: Run, and watch them fail** to compile: `cargo test -p taiyaku-read --all-features -- --test-threads=8`.
- [ ] **Step 3: Implement.**
  - The backend opens the path with `File::open` per call and hands `compress-tools` the `File`.
  - `entries` uses Task 1's listing method and encryption signal.
  - `read` uses `uncompress_archive_file`. `visit` uses `ArchiveIterator` with a filter (`wanted` and readable), so refused entries are skipped, not decoded.
  - Nothing sizes a buffer from a header's size; bytes grow in a `Vec` as they arrive, so `MAX_ENTRY_CAPACITY_HINT` has nothing to clamp. Say so in one line.
- [ ] **Step 4: Run, and watch them pass**, both builds: `cargo test -p taiyaku-read --all-features`, then `cargo test -p taiyaku-read` (no `rar` module, nothing else changes). Then `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo clippy -p taiyaku-read --all-targets -- -D warnings`, `cargo fmt --all -- --check` and `node scripts/check-comments.mjs`.
- [ ] **Step 5: Watched mutations.**
  - (a) `readable` ignores encryption: the kinds test and `an_encrypted_entry…` fail.
  - (b) `visit` ignores `wanted`: the wanted case fails.
  - (c) `visit` goes on after `Err`: the stop test fails.
  - (d) Drop "not a readable RAR archive" from the open error: the truncated test fails.
  - Restore each; `git diff` holds only the intended code.
- [ ] **Step 6: Commit** "taiyaku-read: a private RAR interface and a libarchive backend behind the rar-libarchive feature, not yet reached by any page read".

### Task 3: Page reads go through the interface, and one suite holds every backend

**Files:**
- Modify: `crates/taiyaku-read/src/pages.rs`:
  - `open_archive(path) -> Result<Archive, String>`, where `enum Archive { Zip(zip::ZipArchive<BufReader<File>>), #[cfg(rar)] Rar }` and the sniffed header decides.
  - `unsupported_container`'s `Format::Rar` arm only under `cfg(not(rar))`.
  - `fn is_listed_page(name: &str) -> bool`, extracted from `page_names_from` (the `__MACOSX`, dotfile and `is_page_name` checks) and used by both containers.
  - `#[cfg(rar)] fn rar_page_names(path: &Path) -> Result<Vec<String>, String>`: `backend().entries`, readable only, `is_listed_page`, `natural_cmp` sort.
  - The RAR arms in the table below.
  - Every doc comment's "7-Zip, RAR, or tar" refusal list says RAR is refused only in a build without a RAR backend.
- Modify: `src/format.rs`. One sentence in the module doc and one in `is_readable`'s: in a build with a RAR backend, `pages.rs` can open RAR while this still answers `false`, because what the library admits is decided here, not by what `pages.rs` can open. The O90 record stays as written.
- Modify: `src/pages/tests.rs`. Move the two RAR assertions out of `an_unreadable_container_is_named_by_its_leading_bytes` into `#[cfg(not(rar))] a_rar_header_is_refused_without_a_rar_backend`, and add `#[cfg(rar)] a_rar_header_is_left_to_the_rar_backend` (both headers give `None`). `counting_a_rar_archives_pages_names_the_format` stays in both builds.
- Create: `src/pages/rar_tests.rs` (`#[cfg(all(test, rar))] mod rar_tests;` in `pages.rs`).

| Function | RAR arm |
|---|---|
| `member_page_names` | `rar_page_names` |
| `member_pages` | unchanged: empty names fall to `read_member`'s "no page images" |
| `member_page(i)` | `rar_page_names`, then `get(i)` or `"no page {index}"`, then `backend().read` |
| `archive_page(name)` | `backend().read(path, name)` |
| `member_page_sizes` | the entries' sizes of the listed names, in reading order |
| `member_page_entries` | one `visit` of the listed names; `PageEntry { name, size: bytes.len(), crc32: crc32fast::hash(bytes) }`, in reading order |
| `for_each_member_page` | when the listed names' archive order is the reading order, one `visit`, calling `f` as each page arrives; otherwise `backend().read` per name in reading order. Either way one page in memory at a time and the `PageEntry` built from the bytes |
| `read_member` (`read_archive`) | one `visit`, pages kept by name and returned in reading order; a listed page the visit never handed over is `"{path}: {name}: not read"` |
| `member_entry(name)` | the first readable entry whose whole name `eq_ignore_ascii_case(name)`, read by `backend().read`; none gives `Ok(None)` |

**Interfaces:** Consumes Task 2's `backend()`, `with_backend`, `BACKENDS` and `rar_fixtures()`. Produces nothing new that is public: the public functions keep their signatures.

- [ ] **Step 1: Write the failing tests** in `pages/rar_tests.rs`. Run each over every backend (`with_backend`) and every fixture, with messages naming both.
  - `every_fixture_reads_as_its_manifest_says`, per element:
    - `member_page_names == pages' names` and `member_pages == pages.len()`;
    - for each `i`: `member_page(i)`'s name and sha, and `archive_page(name)`'s sha;
    - `member_page_sizes[i]` equals the `entry` size of that name;
    - `member_page_entries[i] == PageEntry { name, size, crc32fast::hash(bytes member_page returned) }`;
    - `for_each_member_page` hands over that same `PageEntry` sequence, with each page's sha;
    - `read_edition(&[path])`'s names and shas;
    - `member_page(len)` is an error containing `no page {len}`;
    - each `other` read by `member_entry` under its lower-cased name, and `member_entry(path, "missing.xml") == Ok(None)`.
    - `rar5-no-pages.rar`: `member_pages` is an error containing "no page images" (Review Focus 3).
    - Refused fixtures: `member_pages`, `member_page_names` and `member_page(0)` each fail containing "not a readable RAR archive" (Review Focus 2).
  - `the_suite_holds_under_the_c_locale`: run `std::env::current_exe()` with `--exact pages::rar_tests::every_fixture_reads_as_its_manifest_says --nocapture --test-threads=1`, with `LANG`, `LANGUAGE` and every `LC_*` removed and `LC_ALL=C`. Assert the child succeeded and its stdout holds `1 passed`, so a filter that matches nothing cannot pass.
  - `reading_rar_pages_skips_what_it_does_not_need` (`#[cfg(target_os = "linux")]`), for `rar5-stored.rar` and `rar4.rar`. Use `test_support::strace` with trace `read,pread64`, and the child pattern of `listing_an_archive_reads_no_more_for_four_hundred_pages_than_for_four`. In a canonicalised `TempDir`, the child reads page 0 of copy `first`, the last page of copy `last`, every page of copy `stream` through `for_each_member_page`, and copy `plan` through `member_page_entries`. The parent sums the bytes returned on each path with a new `bytes_read_of(log, file) -> u64`, then asserts:
    - `first > 0`;
    - `last − first` is under half the sizes of the entries strictly between the first and the last;
    - `stream` and `plan` are each at most 1.5 × the file's size.
    - The message prints every figure.
  - `a_rar_named_cbz_is_read_as_a_rar`: `rar5-japanese.rar` copied to `x.cbz`; its page names equal the manifest's.
  - `a_zip_member_and_a_rar_member_read_in_order`: `read_members_in_order(&[a.cbz from test_support::cbz_pages, rar5-compressed.rar])` gives the zip's pages, then the RAR's, by name.
  - `a_truncated_rar_names_rar_through_every_function` (Review Focus 1): a half-length copy. `member_pages`, `member_page(0)`, `member_page_entries`, `for_each_member_page` and `member_entry` each fail containing "not a readable RAR archive".
- [ ] **Step 2: Run, and watch them fail:** RAR is still refused by `unsupported_container`.
- [ ] **Step 3: Implement** the dispatch, the extraction and the arms. A comment on `for_each_member_page`'s branch says why: one page in memory, and one decode when the archive already stores pages in reading order.
- [ ] **Step 4: Run, and watch them pass.**
  - `taiyaku-read` in both builds.
  - The whole workspace with `--all-features` (`env -u TAIYAKU_CORPUS cargo test --workspace --all-features -- --test-threads=8`). Other crates' tests write RAR signatures (`match_experiment/tests.rs`, `stamp/tests.rs`, `derive/tests.rs`) and must still pass. A failure there is reported, not fixed by loosening its assertion.
  - The whole workspace in the default build.
  - clippy (both), fmt, `check-comments`.
- [ ] **Step 5: Watched mutations**, each restored:
  - (1) No sort in `rar_page_names`: `rar5-compressed.rar` fails (`p10` first).
  - (2) The readable filter dropped: both encrypted-entry fixtures list `p02.png`.
  - (3) `is_listed_page` bypassed for RAR: `rar5-layout.rar` fails.
  - (4) `for_each_member_page` always streams in archive order: `rar5-compressed.rar`'s order fails.
  - (5) `crc32` set to 0: the entries assertion fails.
  - (6) The child's test name misspelt: `the_suite_holds_under_the_c_locale` fails on `1 passed`.
  - (7) `Libarchive::read` decodes every entry before the wanted one (a full `ArchiveIterator` pass, keeping only the match): the skip assertion fails.
  - (8) `for_each_member_page` always reads per name: the `stream` bound fails on `rar5-stored.rar`.
  - (9) The `cfg(not(rar))` on `unsupported_container`'s RAR arm removed: every fixture is refused under `--all-features`.
  - (10) In the default build, the RAR arm deleted: `a_rar_header_is_refused_without_a_rar_backend` fails.
- [ ] **Step 6: Commit** "taiyaku-read reads RAR pages through the rar interface under rar-libarchive, by the page rules zip follows, and one suite holds every backend to the fixtures, under the C locale too".

### Task 4: Through the crates, CI, the local gate and mutants

**Files:**
- Modify: `crates/taiyaku-library/Cargo.toml` (`rar-libarchive = ["taiyaku-read/rar-libarchive"]`), `crates/taiyaku-serve/Cargo.toml` (adds `taiyaku-library/rar-libarchive`), `crates/taiyaku-cli/Cargo.toml` (adds `taiyaku-serve/rar-libarchive` too). Each gets a one-line why.
- Modify: `.forgejo/workflows/ci.yml`:
  - `lint`: a step before Clippy installing `libarchive-dev` by apt (plus `pkg-config` if Task 1 found the image lacks it), its comment saying why apt and why here, in the voice of the strace step's; the Doc links step becomes `cargo doc --workspace --no-deps --all-features`.
  - `test`: the same packages in the strace step, renamed "Install strace and libarchive's headers".
  - New job `default-features` after `test`, `timeout-minutes: 10`, `rust:1-bookworm`. Steps: the checkout block; nextest from the same pinned tarball and checksum as `test`; strace (O104's tests run here too); "No libarchive in the default build" (see Step 3); `cargo clippy --workspace --all-targets -- -D warnings`; the doc-link check without features; `cargo nextest run --workspace`. It installs no libarchive, and its comment says that is the point.
  - `docker.needs` gains `default-features`. The comments that count the jobs (~:52, ~:496) say five.
- Modify: `.forgejo/workflows/mutants.yml`: the sweep's strace step installs `libarchive-dev` too.
- Modify: `.cargo/mutants.toml`: `all_features = true`, commented: the backend compiles only with its feature, so without it every mutant there would be missed. Check the key against `cargo mutants --help` and the pinned 27.1.0's docs; an unknown key fails loudly.
- Modify: `scripts/check-all.sh`:
  - after the hidden-Rust check, before the lint job, the libarchive check below;
  - the lint section's doc step gains `--all-features`;
  - a `--- the default-features job ---` section after the comment guard, mirroring the CI job, with the test job's nextest/`cargo test` fallback shape. It sits after the status-count check because a later `nextest run` overwrites the JUnit report that check reads;
  - the header's "four jobs" (~:3, :15, :57) becomes five.
- Modify: `docs/ci.md`:
  - the Jobs table: a `default-features` row, and the `lint` and `test` rows' libarchive install;
  - every sentence counting `ci.yml`'s jobs (`rg -n 'four' docs/ci.md`: ~:14, :76, :219, :1965, :1970, :2048): read every hit, and leave a measurement's count as it was;
  - a short `### The default-features job, and why RAR is a feature`: what it proves, and why it installs no libarchive;
  - the mutation section: sweeps build with every feature.
- Modify: `README.md` Contributing (~:818, :825): five jobs, and one sentence that the gate needs libarchive's development files while a plain `cargo build` does not.

**The check-all message, fixed:**

    error: libarchive's development files were not found (pkg-config libarchive).
    The gate builds with --all-features, which turns on taiyaku-read's rar-libarchive
    feature and links libarchive. Install libarchive-dev and pkg-config (Debian, Ubuntu)
    or libarchive and pkgconf (Arch), then run this again. A plain cargo build does not need them.

- [ ] **Step 1: The features.** Make the three manifest edits. Then `cargo tree -p taiyaku-cli -e normal -i compress-tools` finds no `compress-tools` by default, and with `--features rar-libarchive` shows `taiyaku-read` under it. The same holds for `taiyaku-serve` and `taiyaku-library`.
- [ ] **Step 2: The gate's libarchive check.** Add it, then run `env PKG_CONFIG_LIBDIR=/nonexistent PKG_CONFIG_PATH= scripts/check-all.sh --no-wasm --no-web >| $EV/T4-nolib.log 2>&1; echo "exit $?" >| $EV/T4-nolib.exit`. Expected: exit 1, the log's `error:` lines are the message above, and no `==> cargo fmt` line follows.
- [ ] **Step 3: The default-graph guard**: one shell condition, the same in the CI step and in `check-all.sh`. It fails when `cargo tree --workspace -e normal,build,dev --prefix none` (default features) lists `compress-tools`. Watched: add `default = ["rar-libarchive"]` to `taiyaku-read`'s `[features]`, and the guard exits 1 naming `compress-tools`. Restore it.
- [ ] **Step 4: The workflows.** Make the `ci.yml`, `mutants.yml` and `mutants.toml` edits. Parse both workflow files with whatever YAML parser the machine has (python3's `yaml`, or node's from `web/node_modules`), and record which one. The six coverage invocation lines still hash identically between `ci.yml` and `check-all.sh`, by the rule in check-all's comment. `cargo mutants --list --file crates/taiyaku-read/src/rar/libarchive.rs` lists mutants.
- [ ] **Step 5: The docs**, as listed. Then `rg -n -w 'four' README.md docs/ci.md scripts/check-all.sh .forgejo/workflows/ci.yml` and read each hit that counts jobs.
- [ ] **Step 6: Gate** into `$EV/gate-T4.log`: `exit 0` and the `all green` line (unmeasured: the corpus only). If `cargo doc --all-features` fails on a link this branch did not write, stop and report rather than widen the scope. If `status-test-count` fails, set the count it reports at both `status.md` sites and re-run.
- [ ] **Step 7: Commit** "RAR support is built where it is tested: the feature passes through every crate, CI and the gate build with libarchive, a required default-features job keeps the build without it, and mutation sweeps reach the backend".

### Task 5: The image builds the server with RAR support

**Files:**
- Modify: `Dockerfile`.
  - Server stage: a `RUN apt-get update && apt-get install -y --no-install-recommends libarchive-dev …` of its own, after the toolchain install and before `COPY Cargo.toml` so it is cached, with a why comment. Add `pkg-config` only if Task 1 found the image lacks it.
  - The build line gains `--features rar-libarchive`.
  - Runtime stage: `libarchive13` beside `libsqlite3-0`, and its comment now names the two libraries the binary links that the base image lacks.
- Modify: `docs/ci.md`'s "The image job", only if it names the runtime packages (`rg -n 'libsqlite3' docs/ci.md`).

- [ ] **Step 1: Edit**, then build locally, never pushing: `docker build -t taiyaku-rar-check:local $W >| $EV/T5-build.log 2>&1; echo "exit $?" >| $EV/T5-build.exit` → 0.
- [ ] **Step 2: Linkage.**
  - `docker run --rm --entrypoint sh taiyaku-rar-check:local -c 'ldd /usr/local/bin/taiyaku'` shows `libarchive.so.13 => /usr/lib/x86_64-linux-gnu/libarchive.so.13` and no `not found`.
  - The control: `ldd $W/target/debug/taiyaku` after a default `cargo build -p taiyaku-cli` shows no libarchive. So the feature is what links it.
  - `docker run --rm --entrypoint test taiyaku-rar-check:local -f /usr/share/doc/libarchive13/copyright` exits 0: libarchive's notice is in the image.
- [ ] **Step 3: Health.**
  - Start it detached with `--tmpfs /config --tmpfs /cache`, published on a free 127.0.0.1 port: check with `ss -lptn`, and never 4398 or 4399.
  - Within 60 s, `docker inspect -f '{{.State.Health.Status}}'` reads `healthy`, and `curl -fsS -o /dev/null -w '%{http_code}' http://127.0.0.1:<port>/` prints `200`.
  - Stop it by container id, check the port is free, and remove the tag.
- [ ] **Step 4:** `node scripts/check-comments.mjs`. Commit "The image builds the server with rar-libarchive and carries libarchive13; the library still refuses RAR".

### Task 6: The mutation sweep

- [ ] **Step 1: Sweep** the committed branch: `scripts/mutants.sh --since main >| $EV/mutants/sweep-1.log 2>&1` in the background. `--since` diffs commits, so commit first.
- [ ] **Step 2: Read the run for what it can and cannot say.**
  - Its mutants include `rar.rs`, `rar/libarchive.rs` and `pages.rs`.
  - The unviable count makes sense against what changed.
  - At least one mutant in `rar/libarchive.rs` is caught. With the feature off, every one there would be missed.
  - Record all three in `$EV/T6-report.md`.
- [ ] **Step 3: Each survivor in new code** is killed by a test named in the report and watched failing against that mutant, or accepted in `docs/mutants-baseline.md` under one of the reasons its header allows. Commit the tests and the entries ("Tests for the RAR mutants the sweep left alive"), then sweep again (`sweep-2.log`) until only accepted survivors remain.

### Task 7: The ledger and the docs

**Files:** `docs/status.md`, `README.md`, `docs/superpowers/specs/2026-11-17-rar-libarchive-design.md`.

- [ ] **Step 1: O-numbers.** Check main's last row, every lane's claims (paused-frozen O336 and O341, resize-polish O337, the jxl-pages plan's header, content-fingerprints), and the What's next memory. If O380 is taken, renumber every occurrence on the branch to the next free number.
- [ ] **Step 2: Open O380** by the five-part opening (memory `taiyaku-open-items-ritual`):
  - the count word at the head of Open items, read fresh, not trusted;
  - the "N are now closed — leaving M" sentence;
  - the table row, six `|` with `\|` inside code;
  - a prose bullet;
  - the "What remains" line, whose count word wraps: `rg -n -A1 '^What remains:'`.
  - The row: **Reaching a page of a solid RAR decodes every entry before it, and nothing keeps what was decoded.** Site: `rar::libarchive::Libarchive`, `pages::member_page`. Opened by: the RAR design's decision, 2026-11-17. The bullet says what it costs; what would decide it (a solid RAR in a library: none of the owner's 21 is); and the follow-up the design names (unpack a solid archive once, on first open).
  - PR 1 closes no row. A row the review opens and fixes on this branch (O381–O382) is opened, then closed by the five-part close: checkbox, annotation, bullet, both test-count sites, the closed/leaving sentence and What remains.
- [ ] **Step 3: What is built.**
  - The dependency sentence (~:183–200) names `compress-tools` as `taiyaku-read`'s optional dependency behind `rar-libarchive`, linking the system libarchive.
  - The crate table's `taiyaku-read` row (~:405): RAR is read through the private `rar` interface in a build with that feature and refused otherwise; the library and the server refuse it either way, because `Format::is_readable` does.
  - The Rust test count at both sites (~:183, ~:260), from the gate's figure. The web counts do not move: check that.
- [ ] **Step 4: README.** The sentence at ~:441, "Only zip is read", becomes true again: a build with `rar-libarchive`, the image among them, reads RAR members given to the CLI directly. Then read every hit of `rg -n -i '\bRAR\b|\.cbr' README.md docs --glob '!docs/superpowers/**' --glob '!docs/notes.md'` for present-tense statements this branch made false. Closed rows stay as written.
- [ ] **Step 5:** The design's Status line becomes "approved by the owner, 2026-11-17".
- [ ] **Step 6:** Grep the bare `O380` across `docs/` and `web/src/` and read every hit. Run the open-items sweep (rows, closed, What remains).
- [ ] **Step 7: Gate** into `$EV/gate-T7.log` (exit 0, `all green`). Commit "RAR PR 1 in the ledger: what is built, the solid-archive row it opens (O380), and the CLI sentence it made false".

---

### Finish

1. **Whole-branch review** of `main..HEAD` in three lanes, each against the design, the Global Constraints, the Decisions and the Review Focus: (a) Rust, Tasks 2, 3 and 6; (b) build, Tasks 4 and 5; (c) docs, Task 1's notes and Task 7. Fix rounds go back to the implementer, who re-verifies each finding before acting on it. After a fix, grep for the finding's shape, not only its function, and review the fixes too. A fix that adds tests re-measures the count at both sites.
2. **Gate** on the final commit (exit 0, `all green`).
3. **Bookworm, before pushing.** Build a throwaway local image from `rust:1-bookworm` plus `libarchive-dev`. Run `cargo test --locked -p taiyaku-read --all-features -- --test-threads=6` in it by the `taiyaku-sqlite-340-in-ci` memory's recipe (git-archived source, `--user`, the host's 1.100.0 toolchain mounted read-only). Only strace tests may fail there: the memory's three, plus this branch's `reading_rar_pages_skips_what_it_does_not_need`, which then joins the memory's list. Remove the image afterwards.
4. **Claim check** by a fresh subagent: every factual claim in the O380 row and bullet, the status lines, the notes section, the README and `ci.md` sentences, and the PR body. Pass 1 includes the PR body. Corrections go in one commit; later passes are scoped to that commit and the body.
5. **PR.** Title in `$EV/pr-title.txt` (measured under 255 characters), body in `$EV/pr-body.md`. The body covers:
   - what PR 1 does, that the library's derive and scan still refuse RAR, and that a feature build serves a sidecar's RAR members (Decision 8);
   - Decision 8 (the CLI);
   - the spike's four results;
   - the new required check;
   - where the fixtures came from;
   - O380.
   Ask the owner before pushing; open the PR with the gitea MCP. Read the PR's CI, including the advisories workflow (the manifests changed, and `cargo-deny` is not on this machine).
6. **After the owner merges:**
   - Verify the squash: one parent, a tree equal to the branch head's, the body once, no attribution.
   - With the owner's yes, add `default-features`'s pull-request context to `main`'s branch protection by the REST API: GET it, copy the four existing contexts' exact form, PATCH with the fifth, then GET again and compare.
   - Not before the merge: until then no other branch's CI reports that context, and every open PR would wait on it.

### What PR 2 inherits

- `rar::backend()`, the `Backend` trait, the twenty-one fixtures and `test_support::rar_fixtures()`, for library and server tests.
- A feature build already imports and serves a sidecar that names a RAR member (Decision 8): `import::sidecar` and `LocalLibrary::page` check no format. PR 2's switch makes that path intended; its tests cover it with the rest.
- An empty entry in a RAR is refused as "no data" (O381), where a zip's reads as empty, so `member_entry` fails on an empty `ComicInfo.xml` in a RAR member; `taiyaku-cli`'s `credits` reads `ComicInfo.xml` that way and stops on the error. Whatever PR 2 adds that reads `ComicInfo.xml` from a RAR meets the same refusal, and decides whether it means no metadata or a failed member.
- A RAR5 entry that declares only BLAKE2 is read with nothing checking its data (O383, ruling R22 in A9). Whatever PR 2 serves from a RAR meets the same gap.
- libarchive cannot decode three of the owner's good archives (O386, ruling R23 in A9): Fantastic Four v1 360 on either version, and one page each of Zelda v01 and v03 on bookworm's 3.6.2, the image's version. PR 2 settles them before RAR enters a library.
- A lone last part of a multi-volume RAR5 set (a non-goal) lists only the entries that start in it, with no error, dropping a page continued from the part before. Refuse a RAR5 whose main header carries the volume flag; the backend already reads the signature that precedes it.
- Switching it on: `Format::is_readable` under `rar`, with its doc's pointer to this design (the O90 reopening); `Format::PAGED`, if the fingerprint pass is to read a RAR; `scan::pages_of` counting a RAR as it counts a zip.
- The reader signature (built there if the JPEG XL PR has not merged), naming the backend and libarchive's version where Task 1 found the binding exposes it, `compress-tools`'s version otherwise.
- The licence page (`web/src/licenceTexts.ts`) and README's Licence section: libarchive's BSD-2-Clause notice, marked as part of builds with RAR support. Until then the image carries Debian's copyright file (Task 5 Step 2).
- The checks on real material: the 21 RARs in a scratch library, three of which wait on O386; the server's first and last pages, a cover, a resized page and a download; the rollback on a copy of a real store, under O91.
- RAR reading's own row, opened and closed there. O380 stays open.
- `default-features` is a required check once Finish step 6 adds it.

---

## Self-review

- **Spec coverage.**
  - The feature, the alias, the fixed order and the pass-through: Tasks 2 and 4.
  - The three-operation interface in one backend file, and the dispatch on the sniffed format: Tasks 2 and 3.
  - The rules above the interface, and crc32 from the bytes: Task 3.
  - "Without the feature, `unsupported_container` refuses RAR as it does today": Task 3, mutation 10.
  - The fixtures (all seven named, plus three) with manifests: Task 1.
  - The suite over every compiled backend, under `LC_ALL=C` too: Task 3.
  - CI's `--all-features` runs with `libarchive-dev`; mutants with the feature; the required default-features job; the gate's clear failure: Tasks 4 and 6.
  - Dockerfile: Task 5. The spike, its four criteria and the stop: Task 1.
  - "The library and server still refuse it": Global Constraints, checked by `git diff`; but `git diff` cannot see a path that needs no change, and a sidecar's RAR members are served in a feature build (Decision 8).
  - The licence page, the signature, readability, the real-material checks and rollback are PR 2's, listed above.
- **Types.** `Entry`, `Backend`, `Visitor`, `BACKENDS`, `backend()`, `with_backend`, `RarFixture` and `rar_fixtures()` are defined in Task 2 and used under the same names in Tasks 3 and 6. The error texts are fixed once, in Task 2, and Task 3's tests assert them.
- **Review Focus.** 1: Tasks 2 and 3. 2: Tasks 1, 2 and 3. 3: Task 3. 4: Task 2. 5: Task 1.

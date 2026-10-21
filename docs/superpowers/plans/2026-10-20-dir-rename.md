# Folder becomes directory — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Folder" becomes "directory" in prose and "dir" in names, everywhere the spec says, with a clean break for the old flag, variable, routes and addresses, and a server that refuses to start while `TAIYAKU_FOLDERS` is set.

**Architecture:** Rename-only commits first (Rust names, then web names), each proved a pure rename by a word-diff check against a rename map. Then the wire commit (routes, JSON keys, client addresses, flag, variable, the refusal), so the server and the web change their shared names in one commit and every commit runs. Then the prose: user-visible strings, comments, living docs. Last, the mutation baseline's moved keys, a residue grep, and the built app.

**Tech Stack:** Rust (axum, `taiyaku-serve`, `taiyaku-cli`, `taiyaku-library`), React 19 + react-router, vitest (`node`, `jsdom`, `browser` projects), cargo-nextest, cargo-mutants, biome.

**Spec:** `docs/superpowers/specs/2026-10-20-config-grouping-design.md` §5, §6 (the rename). **Branch:** `dir-rename` in the main checkout `/home/davey/projects/taiyaku`, cut from `main` at `72f98caf`, carrying the spec as its first commit. No O-number; the status entry is the Later one, "Next: group the Configuration screens".

**Inventory:** every occurrence, bucketed, with `file:line`, is in `.superpowers/sdd/dir-rename-evidence/inventory.md` (copied there from the session scratchpad before Task 1). Each task's implementer reads its own buckets there rather than re-surveying.

## Global Constraints

- **Prose says "directory"** ("directories", "Directory", "Directories", case kept): sentences, headings, buttons, notices, refusals, CLI help and output, comments, docs. "Add folder…" becomes "Add directory…", "Scan folder" becomes "Scan directory", the Folders heading and table column become "Directories". "Scan library" stays.
- **Names say "dir"**: file names (`folders_api.rs` → `dirs_api.rs`, `FolderPicker.tsx` → `DirPicker.tsx`, `FolderPage.tsx` → `DirPage.tsx`), identifiers, CSS classes (`.folder-table` → `.dir-table`), routes (`/api/library/libraries/{id}/dirs`, `/api/library/dirs`), JSON keys (`folders` → `dirs`), the client address (`/config/libraries/:id/dirs/:did`), `--dirs`, `TAIYAKU_DIRS`.
- **"root" stays.** The table `root`, `root_id`, `RootRow`, `/api/library/roots…` and `--root` name the same rows and are out of scope (the spec renames "folder" only).
- **A clean break:** no alias for `--folders`, `TAIYAKU_FOLDERS`, the old API routes or the old client addresses.
- **Kept as "folder"** (the readings below): dated docs under `docs/superpowers/`, `docs/notes.md`, closed rows in `docs/status.md`, the dated run-log rows of `docs/mutants-baseline.md`, everything inside applied schema migrations (`crates/taiyaku-library/src/schema/*.sql`), the OS's "downloads folder" (`download.rs:11`, `:66`, and the test fn at `download/tests.rs:52`), the fixture directory `"New Folder"` (`libraries_api/tests.rs:1004`), and the refusal's own mention of the old names.
- **A new name must not collide.** Before applying a rename map, list every new name that already occurs in the file (or, for a Rust item, the crate). Each hit is reviewed by hand; where the new name would shadow or clash, choose another and record it. Rust: the shadow lints (Task 1 Step 4) must report no new warning.
- Every new test is watched failing first, against the defect it names. The report says how.
- A whole-page `toContain` cannot see which text belongs where; assert per element.
- Comments say why. No history.
- Gates run whole projects. Rust: `NEXTEST_TEST_THREADS=8 cargo nextest run --workspace >| <log> 2>&1`, then clippy and fmt. Web: `pnpm -C web exec vitest run --project <p> --reporter=default >| <log> 2>&1` for `node`, `jsdom` and `browser`. Read summaries with `grep -a`; never pipe a gate through `tail`. A browser run with import/connection errors and no failed assertion is the headless Chromium crash: check `coredumpctl list --since -1h`, rerun the whole project once.
- Lint (`pnpm -C web run lint`) adds no warning to `main`'s count. Typecheck with the script in `web/package.json`.
- Never `cd` (zoxide). Absolute paths, `git -C`, `pnpm -C web`. Logs in the session scratchpad; maps, check outputs and reports in `.superpowers/sdd/dir-rename-evidence/`. Never the working tree.
- Commits: the message says what the code now does. No attribution lines.
- `evidence/` below means `.superpowers/sdd/dir-rename-evidence/`.

## Readings of the spec (the user answered "1" on 2026-10-20 to "do the six readings hold, and which way should it run")

1. **"Two commits"** (§5) is read as two kinds: rename-only commits (one for Rust, one for the web) that a script can check, and the commits that change words, routes, keys, the flag and the variable.
2. **Wire-bound identifiers wait for the wire commit.** The web's `folders` properties *are* the JSON keys (`libraries.ts:25, 41, 50`), and `createLibrary` sends `{ name, folders }` by shorthand (`:70`); the client route's `:fid` is part of the address. Renaming them in a rename-only commit would split the server from the web, so they move in Task 3 with the server's keys.
3. **The kept "folder"s** listed in the Global Constraints are the readings of "everywhere": the OS's own name for its downloads folder, and an OS default directory name used as a fixture, stay; directories inside an archive ("chapter folders") become "directories".
4. **`docs/status.md`'s living text** includes its current-state sections, not only open rows and Later: the "What is built" file list (`:363`) and the copy of the CLI usage (`:378`) change; closed rows keep.
5. **The mutation sweep** covers the new refusal only. The rest is shown to be a rename by the word-diff check; the baseline's moved keys are re-keyed and each re-read (Task 6), not re-measured.
6. **The word-diff check stands in for §6's lcov diff** on the rename-only commits. It proves no word other than a renamed name changed, which is what the lcov diff was for; a shorter name re-wraps lines, so a per-line lcov comparison would report moved lines, not lost ones. Equal test counts are still required, and the gate's coverage floors still run (Task 6).

## Before Task 1

- [ ] Copy the inventory into `.superpowers/sdd/dir-rename-evidence/`. Run `scripts/check-all.sh` on the branch into a log; record its `all green` line, its exit status, the nextest test count and each vitest project's test count. If it is red, stop and report.
- [ ] Write the word-diff check below to `.superpowers/sdd/dir-rename-evidence/revmap_check.py`. This plan is where it first exists.

```python
#!/usr/bin/env python3
"""Proves a commit is a pure rename: every changed word is old->new in MAP.

usage: revmap_check.py MAP.tsv BASE COMMIT
MAP.tsv holds `old<TAB>new` per line, whole words as the tokenizer splits them
(folder_id, FolderPicker, folder-table). Commas are ignored because rustfmt and biome add or drop a
trailing comma when a shorter name re-wraps a call. Prints each offending
segment with its file; exits 1 if there is any."""
import re, subprocess, sys

# git's regex is POSIX ERE, so the two spellings of one tokenizer differ.
GIT_WORD = r"[A-Za-z0-9_-]+|[^[:space:]A-Za-z0-9_-]"
WORD = r"[A-Za-z0-9_-]+|[^\sA-Za-z0-9_-]"
mapfile, base, commit = sys.argv[1:4]
names = dict(l.rstrip("\n").split("\t") for l in open(mapfile) if l.strip())
out = subprocess.run(
    ["git", "diff", "-M30%", "--word-diff=porcelain", f"--word-diff-regex={GIT_WORD}", base, commit],
    capture_output=True, text=True, check=True).stdout

def words(s):
    return [w for w in re.findall(WORD, s) if w != ","]

bad, path, header, minus = 0, None, True, None
for line in out.splitlines() + ["~"]:
    if line.startswith("diff --git"):
        header = True
        continue
    if header:
        if line.startswith("+++ "):
            path = line[6:]
        header = not line.startswith("@@")
        continue
    if line.startswith("-"):
        minus = (minus or "") + line[1:]
        continue
    plus = line[1:] if line.startswith("+") else ""
    if minus is not None or plus:
        if [names.get(w, w) for w in words(minus or "")] != words(plus):
            print(f"{path}: -{minus!r} +{plus!r}")
            bad += 1
    minus = None
sys.exit(1 if bad else 0)
```

Prove the check itself before relying on it: on a scratch commit that renames one identifier *and* changes one string literal, it must print the string's segment and exit 1; with the string change reverted it must exit 0. Record both runs.

---

### Task 1: Rust names, rename only (spec §5, "Names say dir")

**Files:**
- Rename: `crates/taiyaku-serve/src/folders_api.rs` → `dirs_api.rs`, `folders_api/` → `dirs_api/` (`lib.rs:31` `mod`, `lib.rs:401`, `routes.rs:2321`)
- Modify: every Rust file in inventory §2a and §2b (production and test identifiers, including the 148 test fn names), rustdoc intra-doc links naming those identifiers (the `broken_intra_doc_links` gate reads them), backticked identifier mentions in comments, and inline format captures such as `{folder}` (output unchanged)
- Do not touch: route path literals, `json!` keys, any field of a `Serialize`/`Deserialize` type (check: the inventory found only hand-written `json!` keys; confirm with `git grep -n -i 'derive.*Serialize\|derive.*Deserialize'` over the files touched), user-visible strings, prose in comments, the `--folders` and `TAIYAKU_FOLDERS` literals, fixture titles

- [ ] **Step 1:** Generate the map from the code: every Rust identifier matching `/folder/i`, its new name by `folder→dir`, `Folder→Dir`, `FOLDER→DIR` (so `folders_of` → `dirs_of`, `View::Folders` → `View::Dirs`, `is_folder` → `is_dir`). Save as `evidence/rust-map.tsv`.
- [ ] **Step 2:** Collision review: for each new name, `git grep -n -w <new>` over its crate on the base commit. Review every hit's scope; adjust the map where a new name would shadow or clash, and list each adjustment in the report.
- [ ] **Step 3:** Apply the map, rename the files, `cargo fmt --all`.
- [ ] **Step 4:** On the base commit and on the result, run `cargo clippy --workspace --all-targets --all-features -- -W clippy::shadow_unrelated -W clippy::shadow_reuse -W clippy::shadow_same` into two logs; the result must add no shadow warning that names a renamed identifier. Then the normal clippy (`-D warnings`), the doc gate (`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps`) and nextest over the workspace: green, and the test count equal to the baseline's.
- [ ] **Step 5:** Commit ("…: Rust code names a directory dir"), then `revmap_check.py evidence/rust-map.tsv HEAD~1 HEAD` exits 0. Save its output.

### Task 2: Web names, rename only (spec §5, "Names say dir")

**Files:**
- Rename: `web/src/FolderPicker.tsx` → `DirPicker.tsx`, `FolderPicker.test.tsx` → `DirPicker.test.tsx`, `FolderPage.tsx` → `DirPage.tsx`, `FolderPage.test.tsx` → `DirPage.test.tsx` (importers: `LibraryPage.tsx:49`, `NewLibrary.tsx:16`, `Libraries.tsx:65`, `Config.browser.test.tsx:17`)
- Modify: every TS identifier in inventory §2a and §2b; CSS classes and `name` attributes in §2c (`admin.css` selectors and the TSX that uses them together), with the lists that pin them: `classNames.test.ts:127` (`HOOKS`, `'folder-table'`), `admin-config.css.test.ts:119, 215-218, 274, 278`
- Do not touch: the `folders` properties of `Library`, `LibraryScanStatus` and `FolderListing` (`libraries.ts:25, 41, 50`; the type `FolderListing` itself is renamed `DirListing`), `createLibrary`'s `folders` parameter (`:70`, a shorthand key), fetch paths, the client route `:id/folders/:fid` and its `fid`, user-visible strings, prose, fixture data

- [ ] **Step 1:** Generate `evidence/web-map.tsv` the same way (whole identifiers, class names, import paths such as `./FolderPicker`).
- [ ] **Step 2:** Collision review as in Task 1 Step 2, per file.
- [ ] **Step 3:** Apply, rename the files, format with biome.
- [ ] **Step 4:** Typecheck, lint (no new warning), and the `node`, `jsdom` and `browser` projects: green, each project's test count equal to the baseline's.
- [ ] **Step 5:** Commit ("…: web code names a directory dir"); `revmap_check.py evidence/web-map.tsv HEAD~1 HEAD` exits 0.

### Task 3: Routes, keys, addresses, flag and variable; the refusal (spec §5, "A clean break")

**Files:**
- Server: `routes.rs:2309-2310`, `:2320-2321` (the two routes), the grant table `:2795-2799`, `:2811`, and the hand-typed route lists `auth/tests.rs:137-139`, `:3499-3501`; `json!` keys at `libraries_api.rs:114`, `:146` (`body.get("folders")`), `:500`, `dirs_api.rs` (was `folders_api.rs:415`), and the refusals naming the field (`libraries_api.rs:149`, `:154`); server tests on those routes and keys (inventory §7a, "wire")
- Web: `libraries.ts` fetch paths (`:85`, `:121-122`), the three `folders` properties and `createLibrary`'s shorthand; the 30 `.folders` reads (inventory §3); `Libraries.tsx:418` (`:id/dirs/:did`) and its `useParams` (`:494`); the link at `LibraryPage.tsx:475`; the ~47 test addresses and the fixtures' `folders:` keys
- CLI and image: `--folders` → `--dirs` in `main.rs` (`reject_unknown_flags` list at `:1243`, the parse, the usage text) and the flag-list assertion `cli/src/tests.rs:2569`; `TAIYAKU_FOLDERS` → `TAIYAKU_DIRS` at `main.rs:1366` and its doc lines, `Dockerfile:88-92`, `crates/taiyaku-serve/src/lib.rs:182`, `dirs_api.rs` doc lines, the UI sentence at `DirPicker.tsx` (was `FolderPicker.tsx:272`) and its test, README's lines on the flag and variable (`:264`, `:475-490`), and `cli/tests/cli.rs:1641`
- Tests: `cli/src/tests.rs:3175-3223`

**Interfaces:**
- Produces: `GET /api/library/dirs[?path=…]` answering `{"dirs": [{name, path}]}`; `POST /api/library/libraries/{id}/dirs`; `POST /api/library/libraries` taking `{name, dirs}`; each library's `dirs` array and scan status's `dirs[].root_id`. PR 2 builds on these names.

- [ ] **Step 1: The refusal, test first.** Beside the existing environment tests (`cli/src/tests.rs:3175-3223`), add: with `TAIYAKU_FOLDERS=/media` in the environment, `serve` refuses before binding, and the error names both variables. Watch it fail on the unchanged code (today the old name is read, not refused). The sentence, new here: *"TAIYAKU_FOLDERS was renamed TAIYAKU_DIRS; rename it where the server is configured"*. It refuses whenever `environment("TAIYAKU_FOLDERS")` would have returned a value, so a setting that had an effect before cannot be dropped silently; `--dirs` on the command line does not excuse it.
- [ ] **Step 2:** The near-miss test at `:3179` (today: `TAIYAKU_FOLDER` and `FOLDERS` are not read) becomes the same check for `TAIYAKU_DIR` and `DIRS`. Add: `serve --folders /x` is refused as an unknown flag, watched failing against a hand-added `--folders` alias.
- [ ] **Step 3:** Implement the refusal next to the read at `main.rs:1366`, shaped like the other `serve_options` refusals.
- [ ] **Step 4:** Rename the routes, keys, addresses, flag and variable everywhere listed, server and web together.
- [ ] **Step 5:** An old client address (`/config/libraries/1/folders/1`): see what the Libraries routes render for it. If it is a blank pane rather than the existing not-found treatment, report it with a screenshot. Do not add a route.
- [ ] **Step 6:** The full Rust gate and all three web projects, green. Commit ("…: routes, keys, addresses, --dirs and TAIYAKU_DIRS say dir, and the server refuses TAIYAKU_FOLDERS").

### Task 4: What people read (spec §5, "Prose says directory")

**Files:**
- Modify: the user-visible strings in inventory §5: 44 in 13 TS files, 75 in 12 Rust files (refusals, notices, late-answer sentences, log lines a user sees), the CLI's output sentences (`library.rs`, 5) and help text, the grant reasons in `routes.rs` (`:2759`, `:2795-2799`, `:2806-2807`), and the OPDS root entry `opds.rs:144` ("Each library, with the works under its directories"); every test that asserts one of these strings (inventory §7a, "pinned"), and the fixture titles and names ("First Folder Work", "Second Folder Work", "Folderless")

- [ ] **Step 1:** Change each string by hand, not by a blind replace: read each sentence after the change for grammar and for the kept senses (Global Constraints).
- [ ] **Step 2:** Update the tests that pin those strings to the new words. A string with no test that is shown to the user in the web app gets none added here; the residue grep (Task 6) is its check.
- [ ] **Step 3:** The Rust gate and all three web projects, green. Commit ("…: the web app, the server and the CLI say directory").

### Task 5: Comments (spec §5)

**Files:**
- Modify: the comment occurrences in inventory §6 (Rust `//`, `///`, `//!`; TS and CSS comments), production and tests, except inside `schema/*.sql` and the kept senses

- [ ] **Step 1:** A case-keeping replacement over comment text only (`folder→directory`, `folders→directories`, `Folder→Directory`, `Folders→Directories`), then read the whole diff: fix grammar, restore any kept sense, and catch any identifier mention the replacement turned into prose (`directory_id` where the code says `dir_id`).
- [ ] **Step 2:** The doc gate, clippy, fmt, lint; nextest and the three web projects. Commit ("…: comments say directory").

### Task 6: Docs, the baseline, the residue, the built app (spec §5, §6)

**Files:**
- Modify: `README.md` (remaining prose), `docs/sidecar.md:326, 331`, `docs/status.md` (open rows, Later, `:363`, `:378`; the open row O190 names a test fn renamed in Task 1), `docs/mutants-baseline.md` (the moved keys, and the accepted survivor's reason at `:644-645`, which names `in_folders`); the Later entry "Next: group the Configuration screens…" now names the spec and says PR 1 (this rename) shipped and PRs 2 and 3 are next

- [ ] **Step 1: The baseline.** For each of the 11 files with baseline entries (inventory §9), run `cargo mutants --list --file <f>` on `main` and on the branch; pair each baseline entry with its new `file:line:col: description` by the description (with renamed names mapped) and order. Re-read each moved entry's reason against the code it now points at, and update the key. The count of entries is unchanged. Dated run-log rows (`:155-156`) stay.
- [ ] **Step 2: The mutation sweep** over the refusal only (`scripts/mutants.sh` scoped to `main.rs` and the refusal's function). Every survivor is killed or written into the baseline with its reason.
- [ ] **Step 3: The residue.** `git grep -n -i -I folder -- . ':!docs/superpowers/' ':!docs/notes.md' ':!crates/taiyaku-library/src/schema/*.sql' >| evidence/residue.txt`. Classify every line in the report: a closed row of `status.md`, a dated run-log row of `mutants-baseline.md`, a kept sense from the Global Constraints, or the refusal and its tests naming the old names. Anything else is fixed.
- [ ] **Step 4: The built app.** Stop the prototype server on `127.0.0.1:4399` (by PID, after `ss -lptn 'sport = :4399'`; it serves the proto branch). Build this branch's binary and web into a fresh scratch dir with a copy of the test library's database, and serve with `TAIYAKU_DIRS=<scratch>/media` holding two empty subdirectories. Check, with screenshots in Paper and Ink: Libraries → a library shows "Directories" and "Add directory…", and the picker lists the two subdirectories; a directory's page is at `/config/libraries/<id>/dirs/<did>` with "Scan directory"; the old `/folders/` address behaves as Task 3 Step 5 reported. Then start the binary with `TAIYAKU_FOLDERS` set and record the refusal. Do not press Scan or tick anything that writes under a real directory. Afterwards: kill by PID, `find /home/davey/temp/manga-test -newer <marker>` empty, the library's digest unchanged.
- [ ] **Step 5: The gate.** `scripts/check-all.sh` into a log; quote its `all green` line and exit status. Coverage floors: report any that moved.
- [ ] **Step 6:** Commit the docs. Write the PR body to `evidence/pr-body.md`: what was renamed and what was kept; **the deployed container's `TAIYAKU_FOLDERS` must be renamed `TAIYAKU_DIRS` in the same update, or the server refuses to start**; `--folders` is now `--dirs`; the old API routes and `/folders/` addresses no longer answer.

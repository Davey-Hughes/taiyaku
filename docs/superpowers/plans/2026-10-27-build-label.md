# A local build names its commit: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A build without `TAIYAKU_COMMIT` names git's HEAD in the version label and on About, marked with `*` where tracked files differed from HEAD when it was built.

**Architecture:** A build script in `taiyaku-serve` gathers facts from git (HEAD, whether tracked files changed) and hands them to the crate as compile-time env vars; it decides nothing. `about.rs` decides which source wins and answers `commit` and a new `dirty` from `/api/about`; the web draws the star and the words.

**Tech Stack:** Rust (cargo build script, axum route), React + TypeScript (vitest node/jsdom/browser projects).

**Spec:** `docs/superpowers/specs/2026-10-26-build-label-design.md`. Read §1–§4 before Task 1, §3 again before Task 2, §5 before Task 3. It is the record of the user's rulings; this plan does not repeat its reasons.

## Global Constraints

- Order: a full 40-character lowercase hex `TAIYAKU_COMMIT`, else git's HEAD, else no commit ("unknown build"). An invalid `TAIYAKU_COMMIT` falls through to HEAD. A commit from `TAIYAKU_COMMIT` is never marked changed.
- Changed means `git status --porcelain --untracked-files=no` prints anything; untracked files never count.
- The build script reruns on git's state and the Rust inputs only (spec §3.3); it never names a path that does not exist; it never writes to anything it watches.
- Strings, exactly: label `abc1234*`; its name "About, build abc1234, with uncommitted changes"; About's row: the linked short commit, then " with uncommitted changes". Unchanged builds keep today's strings ("About, build abc1234", "unknown build", "About, unknown build").
- `/api/about` keeps `commit` (hash or `null`) and adds `dirty` (boolean, `true` only with a commit).
- Comments say why only: no history, no measurements (`CLAUDE.md`). Measurements go in the task report and later the PR body.
- Workspace lints apply to `build.rs` too (`[lints] workspace = true`, clippy pedantic): it must pass the gate's clippy step.
- Shell: the Bash tool runs zsh with `noclobber` (`>|` to start a log), `grep` is a function that skips binary files (`grep -a`), `cd` is zoxide (use absolute paths and `git -C`). Cargo tests: `NEXTEST_TEST_THREADS=8`. vitest: `--reporter=default`. A probe worktree gets its own `CARGO_TARGET_DIR` and a **copied** `web/node_modules`, never a symlink.

---

### Task 1: The server decides which source wins and answers `dirty`

**Files:**
- Modify: `crates/taiyaku-serve/src/about.rs` — `build_commit` (~:9-23) becomes `full_hash` with its doc kept; a `BuildLabel` and `build_label` beside it; the route (~:46-48) reads all three env vars and answers `dirty`.
- Test: `crates/taiyaku-serve/src/about/tests.rs` — the `build_commit` cases (~:3-47) move to `full_hash`; new `build_label` cases; the route case (~:59-70) expects `dirty`.

**Interfaces:**
- Produces, for Task 2's docs and for the route:

```rust
/// What `/api/about` says about the build: the commit it was built from, and
/// whether its tracked files differed from that commit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BuildLabel {
    pub(crate) commit: Option<&'static str>,
    pub(crate) dirty: bool,
}

/// `TAIYAKU_COMMIT` when it is a full hash, never marked; otherwise git's HEAD
/// as the build script read it, marked when the script saw changes; otherwise
/// no commit.
pub(crate) fn build_label(
    commit: Option<&'static str>,
    head: Option<&'static str>,
    dirty: Option<&'static str>,
) -> BuildLabel {
    if let Some(commit) = full_hash(commit) {
        return BuildLabel { commit: Some(commit), dirty: false };
    }
    match full_hash(head) {
        Some(head) => BuildLabel { commit: Some(head), dirty: dirty.is_some() },
        None => BuildLabel { commit: None, dirty: false },
    }
}
```

  The route calls `build_label(option_env!("TAIYAKU_COMMIT"), option_env!("TAIYAKU_GIT_HEAD"), option_env!("TAIYAKU_GIT_DIRTY"))` and answers `"commit": label.commit, "dirty": label.dirty`. The env var names `TAIYAKU_GIT_HEAD` and `TAIYAKU_GIT_DIRTY` are Task 2's contract: HEAD's full hash, and `1` when changed (absent when not).

- [ ] **Step 1: Write the `build_label` cases** in `about/tests.rs`, one `#[test]` each, named for what they hold, using the file's `FULL` and a second full hash for HEAD:
  - a valid `TAIYAKU_COMMIT` wins over a valid, changed HEAD, and is not marked;
  - an invalid `TAIYAKU_COMMIT` (`NOT_HEX`) falls through to a changed HEAD: HEAD, marked;
  - no `TAIYAKU_COMMIT`, HEAD clean: HEAD, not marked;
  - neither, with the dirty fact set: no commit, not marked;
  - an invalid HEAD (`UPPER`): no commit, not marked.
  Rename the `build_commit` cases to `full_hash`. Change the route case to build its expected `commit` and `dirty` from the same `build_label(option_env!(…))` call the route makes.
- [ ] **Step 2: Red.** Write `build_label` as the old behaviour (`BuildLabel { commit: full_hash(commit), dirty: false }`) and run `NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve about >| <log> 2>&1`. Expected: the fall-through and HEAD cases fail. Record the failing names.
- [ ] **Step 3: Green.** Write `build_label` as in Interfaces, and the route as described. Update `about.rs`'s doc on the route (~:31-34) only as far as this task makes it false: it now reads three values; leave the "nothing runs git" sentence for Task 2, which makes it false.
- [ ] **Step 4:** Run the same command; expected all pass. Run the gate's clippy step (read `scripts/check-all.sh` for its exact invocation) on `taiyaku-serve`.
- [ ] **Step 5: Commit** (message says what the code now does; no attribution lines).

### Task 2: The build script gathers git's facts

**Files:**
- Create: `crates/taiyaku-serve/build.rs`
- Modify (docs, each made true for this): `about.rs`'s route doc (~:31-34: a build script runs git; the image's context holds no `.git`, so there the argument is the only source); `Dockerfile` (the comment above `ARG TAIYAKU_COMMIT`, ~:59-64); `README.md` (the build label's section, ~:584-596: a local `cargo build` names HEAD itself, with `*` for a changed tree, worked out again on a commit, checkout, stage or Rust edit (spec §3.4 says what that misses); `TAIYAKU_COMMIT` still wins, and an image build still needs it; keep both commands); `.forgejo/workflows/ci.yml` (the `test` job's comment, ~:254-257: unset, the route test compares with what the build script read, not `null`); `docs/ci.md` (~:16-18, the same; and "The image job", ~:1704-1707: the image's build runs no git that finds a repository).

**Interfaces:**
- Consumes: nothing from Task 1 at build time. Produces the env vars Task 1 reads: `cargo:rustc-env=TAIYAKU_GIT_HEAD=<hash>` and, when changed, `cargo:rustc-env=TAIYAKU_GIT_DIRTY=1`.

- [ ] **Step 1: Write `build.rs`.** It is new code, so here it is; adjust only where clippy or the checks below require it, and say why in the report.

```rust
//! Hands `about.rs` what git says about the tree this crate is built from:
//! HEAD's commit, and whether tracked files differ from it. Which source the
//! server reports is decided in `about::build_label`, not here.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"),
    );
    let root = manifest
        .join("../..")
        .canonicalize()
        .expect("the workspace root exists");
    watch_rust_inputs(&root);
    if let Some(facts) = git_facts(&root) {
        for path in &facts.watch {
            watch(path);
        }
        println!("cargo:rustc-env=TAIYAKU_GIT_HEAD={}", facts.head);
        if facts.dirty {
            println!("cargo:rustc-env=TAIYAKU_GIT_DIRTY=1");
        }
    }
}

struct Facts {
    head: String,
    dirty: bool,
    watch: Vec<PathBuf>,
}

/// Every fact or none: a commit reported without its mark would be a wrong
/// label, and no label is better than a wrong one.
fn git_facts(root: &Path) -> Option<Facts> {
    // A source tree unpacked inside another repository must not report that
    // repository's HEAD.
    let top = PathBuf::from(git(root, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .ok()?;
    if top != root {
        return None;
    }
    let head = git(root, &["rev-parse", "HEAD"])?;
    let dirty = !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    let git_dir = within(root, git(root, &["rev-parse", "--git-dir"])?);
    let common = within(root, git(root, &["rev-parse", "--git-common-dir"])?);
    let mut watch = vec![
        git_dir.join("HEAD"),
        git_dir.join("index"),
        common.join("packed-refs"),
    ];
    // Detached, `symbolic-ref` fails and there is no branch ref to watch.
    if let Some(branch) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
        watch.push(common.join(branch));
    }
    Some(Facts { head, dirty, watch })
}

/// `git -C root …`'s output, or `None` where git is missing or refuses.
///
/// A hook's environment would point git at another index or repository, so
/// those variables are dropped. `GIT_OPTIONAL_LOCKS=0` stops `git status`
/// refreshing the index: the index is watched, and a script that wrote it
/// would mark itself stale and run again on the next build.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim_end().to_owned())
}

/// git answers `--git-dir` relative to `root` where it can.
fn within(root: &Path, answer: String) -> PathBuf {
    let path = PathBuf::from(answer);
    if path.is_absolute() { path } else { root.join(path) }
}

/// Cargo treats a path that does not exist as changed, which would rerun this
/// script on every build.
fn watch(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// Each crate's `src` and manifest, not `crates/` whole: test runs and wasm
/// builds write ignored files under it (`taiyaku-cli/.taiyaku-cache`,
/// `taiyaku-wasm/pkg`), and each write would rerun this and recompile the
/// crate.
fn watch_rust_inputs(root: &Path) {
    watch(&root.join("Cargo.toml"));
    watch(&root.join("Cargo.lock"));
    if let Ok(crates) = std::fs::read_dir(root.join("crates")) {
        for entry in crates.flatten() {
            watch(&entry.path().join("Cargo.toml"));
            watch(&entry.path().join("src"));
        }
    }
}
```

- [ ] **Step 2:** `cargo build -p taiyaku-cli`, then read the newest `target/debug/build/taiyaku-serve-*/output`: expect `TAIYAKU_GIT_HEAD=` equal to `git rev-parse HEAD`, and the `rerun-if-changed` lines (no path listed that does not exist). Run Task 1's tests again, and the gate's clippy step.
- [ ] **Step 3: The hand checks (spec §7), each recorded in the report with its command and output.** Where a check changes files, do it in a probe worktree (`git worktree add <scratch>/probe HEAD`) and run the compiled script there directly: `CARGO_MANIFEST_DIR=<probe>/crates/taiyaku-serve OUT_DIR=<scratch>/out target/debug/build/taiyaku-serve-<hash>/build-script-build` prints its `cargo:` lines with no build. Restore everything and remove the probe at the end; the main checkout's tree must be as Task 1 left it plus this task's files.
  1. **Fresh:** a second `cargo build -p taiyaku-cli -v` with nothing changed reports `taiyaku-serve` fresh; `stat -c %Y` of the git dir's `index` is the same before and after the first build.
  2. **Changed:** an unstaged edit under the probe's `crates/*/src` gives `TAIYAKU_GIT_DIRTY=1`; committing it in the probe (on a throwaway detached commit) takes it away.
  3. **Worktree:** in the probe, HEAD and the mark are the probe's, and the watched `HEAD`/`index` are under `.git/worktrees/<name>/`.
  4. **Not this repository:** `git archive HEAD` unpacked inside a fresh `git init` directory: the script prints no `TAIYAKU_GIT_*` line.
  5. **`TAIYAKU_COMMIT`:** `TAIYAKU_COMMIT=$(git rev-parse HEAD~1) cargo build -p taiyaku-cli -v` rebuilds `taiyaku-serve` (cargo names the env var as the reason); the decision itself is Task 1's unit test.
  6. **Watched mutation:** delete the `GIT_OPTIONAL_LOCKS` line, `touch` a tracked file without changing it, build twice: expect the second build to rerun the script (cargo `-v` names `index` as changed). If git does not write the index here, record that and why. Restore the line.
  - **Measure and record:** a no-op `cargo build -p taiyaku-cli` before and after this task; the cost of one rerun (stage and unstage a docs file, then build); and whether a plain `git status` after editing a file under `docs/` makes the next build rerun the script (an editor's or a prompt's git status can refresh the index).
  - **Mutation harness:** run `cargo mutants --list` as `scripts/mutants.sh` configures it and record whether `build.rs` gets mutants. If not, the hand checks are its only test; say so in the report.
- [ ] **Step 4: The docs** listed under Files, each sentence true after this task. Comments why only.
- [ ] **Step 5:** Run `taiyaku-serve`'s tests whole and the clippy step. Commit.

### Task 3: The web draws the mark, and status.md says what shipped

**Files:**
- Modify: `web/src/about.ts` (the `About` type ~:24-30: `dirty: boolean`, with a one-line doc saying it is true only with a commit); `web/src/testing/about.ts` (`ABOUT` and `UNKNOWN_BUILD` gain `dirty: false`; add `DIRTY_BUILD = { ...ABOUT, dirty: true }` with a doc line); `web/src/VersionLabel.tsx` (~:35-55: the text and the name for a changed build, per Global Constraints; its file doc ~:1-5); `web/src/About.tsx` (~:74-80: " with uncommitted changes" after the link; its doc ~:17-19).
- Test: `web/src/VersionLabel.test.tsx` (beside "reads \"unknown build\"…" ~:123) and `web/src/About.test.tsx` (beside ~:69), one case each for a changed build, reading the text and the accessible name per element; and check the existing committed-build cases still assert there is no star and no words.
- Docs: `docs/status.md` — the Later entry "Next after PR 3: a local build names its commit" (~:4868-4880) is removed, and the About entry's sentences on the commit (~:5711-5725, "**The commit** is compiled in from `TAIYAKU_COMMIT` through `option_env!`…" and "**The label** … is the short commit or \"unknown build\"") say what now ships: the order, the build script, `dirty`, the star and the words. Grep `docs/` and `README.md` for "unknown build" and `TAIYAKU_COMMIT` and fix any other sentence this makes false. Update "What is built"'s test counts (~:149-151) from whole-project runs at your commit.

**Interfaces:**
- Consumes: `/api/about`'s `dirty` (Task 1).

- [ ] **Step 1:** Add the fixtures and the two test cases.
- [ ] **Step 2: Red.** Run the two files' project (`.test.tsx` here is jsdom: confirm in `web/vitest.config.ts`); expected: the new cases fail on today's code. Record.
- [ ] **Step 3:** Implement the type, the label and the row.
- [ ] **Step 4: Watched mutations.** Hand-apply each and watch its case fail, then restore: the star dropped from the label's text; the words dropped from its name; the words dropped from About's row; the star drawn for a clean build.
- [ ] **Step 5:** status.md and any other doc, as listed.
- [ ] **Step 6:** Run the node, jsdom and browser projects whole, typecheck and lint; record the counts. Commit.


### Task 4: The sidebar foot keeps links out of the bottom-left corner (added 2026-10-27 after the user's look)

**The ruling (the user, 2026-10-27, looking at the two builds):** "in browsers, they show link previews when hovering, and it goes over where the commit hash link is. firefox keeps trying to put the link preview on the right side but it's not consistent … maybe we can reorder the items in the bottom corner". Offered three orders, they chose **the label, then More, then Theme**: the label at the top of the foot, Theme last, so that a control holding no link fills the corner where a browser draws its link preview, and the label and More sit above it.

**Files:**
- Modify: `web/src/Shell.tsx` (~:370-374, `.sidebar-foot`: `VersionLabel`, then `AccountMenu`, then `ThemeControl`); `web/src/styles/shell.css` (`.sidebar-foot` ~:45 and whatever spaces the three, so the gaps read as they did, the label quiet above More; the column and the drawer alike).
- Test: the Shell/sidebar tests that read the foot's order or its focus order (grep for `sidebar-foot`, `ThemeControl`, `VersionLabel`, `AccountMenu` in `web/src/*.test.tsx`), and a browser case.
- Docs: `docs/status.md` ~:5522 ("at its foot Theme above More") and ~:5751 ("The label at the sidebar's foot, under More"); the sidebar spec `docs/superpowers/specs/2026-10-18-sidebar-readability-about-design.md` (~:63-64 and ~:204-207), each saying the new order and why in one clause; this branch's spec §5 gains the ruling. Grep `README.md` and `docs/` for any other sentence on the foot's order.

- [ ] **Step 1: Red.** A browser case, in the column at 1280 and in the open drawer at 390: the foot's three items in visual order top to bottom are the label, More, Theme; Theme's box ends at the foot's bottom; the label's and More's boxes end above Theme's top. Watch it fail on today's order.
- [ ] **Step 2:** Reorder, and adjust the CSS so nothing else in the sidebar moves: the library list, the wordmark and the foot's top edge stay where they were, or say what moved and why.
- [ ] **Step 3: More's menu.** It opens upward. Open it in both placements and check it draws whole and over the label, never under it, and that Escape and a click outside still close it.
- [ ] **Step 4: Focus order.** Tab through the foot: the label, More, then Theme's controls. Update any test that pinned the old order, and check that the drawer's focus cases (opening focuses the Libraries heading's link, Close one Shift+Tab back) still hold.
- [ ] **Step 5:** Docs as listed. Run the node, jsdom and browser projects whole, typecheck and lint; commit.

---

## After the tasks (controller)

1. Whole-branch review (one reviewer; the branch is small), then one fix round if needed and a review of it.
2. `scripts/mutants.sh --since main` over the Rust; read the result per the harness memory (survivors, unviable count, errored runs).
3. The full gate, `scripts/check-all.sh`, into a log; quote its `all green` line and exit status.
4. The user's look: a test server built from the branch with an uncommitted change, showing `abc1234*` and About's words; then the same tree committed, showing none.
5. Claim check over the docs this branch changes and the PR body (which records Task 2's measurements and hand checks); then the push and PR asks, each separately.

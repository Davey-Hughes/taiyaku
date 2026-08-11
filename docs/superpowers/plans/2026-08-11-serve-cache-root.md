# A cache root that does not follow the working directory — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `taiyaku serve`'s cover, tile and sized-page cache off the
relative `.taiyaku-cache/covers` and onto a platform cache root, so where it
lands no longer depends on where the server was launched from.

**Architecture:** One new private module in `taiyaku-serve` holds a pure
resolver — `cache_root(flag, xdg, home) -> Resolution` — that `serve()` calls
with the results of two `env::var_os` reads. Its `covers()` leaf becomes
`ServeState::cache_dir`, and `thumb_cache_dir()`'s existing `with_file_name`
derivation carries the tiles along unchanged. `taiyaku-cli` gains a `--cache
DIR` flag that feeds the resolver's first argument.

**Tech Stack:** Rust 2024 edition, MSRV 1.85; `axum` + `tokio` in
`taiyaku-serve`; tests via `cargo test` / `cargo nextest`; gate via
`scripts/check-all.sh`.

**Spec:** `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md`

## Global Constraints

- **No new dependencies.** The workspace's runtime dependency list is short
  enough to be quoted in `docs/status.md`; this is five lines of `std`.
- **No `std::env::set_var` in any test.** It is `unsafe` in edition 2024 and
  racy under nextest's threads. The resolver takes its environment as
  arguments for exactly this reason.
- **No test asserts a real home directory.** A test that reads `$HOME` passes
  on one machine and says nothing about any other.
- **`cargo clippy --workspace --all-targets --all-features -- -D warnings`
  must pass.** `clippy::pedantic` is advisory and non-blocking, but a new
  `#[expect]` needs a `reason = "…"` stating its bound if one becomes
  necessary.
- **Rust coverage gate:** `--fail-under-lines 93`, a per-file floor of 57%
  lines, and a minimum of 32 files. A new module is a new file — the file
  count moves up, never down.
- **No attribution in commit messages.** No `Co-Authored-By`, no
  `Generated with`.
- **Doc comments carry the argument, not just the description.** Every
  decision in this codebase records the alternative it beat. Match that.

---

## File Structure

| File | Responsibility | Task |
| ---- | -------------- | ---- |
| `crates/taiyaku-serve/src/cache_root.rs` | **new** — resolve a cache root from a flag and two environment variables, and report which rule fired | 1 |
| `crates/taiyaku-serve/src/lib.rs` | declare the module; `serve()` reads the environment, warns on the fallback branch, and sets `cache_dir` from the result | 2 |
| `crates/taiyaku-cli/src/main.rs` | `--cache DIR` on `serve`: usage line, accepted-flag list, value read, pass-through | 3 |
| `crates/taiyaku-cli/tests/cli.rs` | the two refusal contracts for the new flag | 3 |
| `docs/sidecar.md`, `docs/status.md` | five documentation sites the change makes false | 4 |

---

### Task 1: The resolver

A pure function and its six tests. Nothing else in the crate knows it exists
yet, so this task is complete and reviewable on its own.

**Files:**
- Create: `crates/taiyaku-serve/src/cache_root.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub(crate) enum Resolution { Flag(PathBuf), Xdg(PathBuf), Home(PathBuf), Fallback(PathBuf) }`
  - `pub(crate) fn cache_root(flag: Option<&Path>, xdg: Option<&OsStr>, home: Option<&OsStr>) -> Resolution`
  - `pub(crate) fn Resolution::root(&self) -> &Path`
  - `pub(crate) fn Resolution::covers(&self) -> PathBuf`

- [ ] **Step 1: Create the file with the tests only**

Create `crates/taiyaku-serve/src/cache_root.rs` containing exactly this. The
`use super::*;` will not resolve yet — that is the point of step 2.

```rust
//! Where the served cache lives, decided independently of the working
//! directory.
//!
//! `ServeState::cache_dir` used to be the relative `.taiyaku-cache/covers`, so
//! a server started from a different directory built a second cache and
//! regenerated every page into it — 126 ms median a page against 10 ms warm,
//! measured over 235 real generations. This module is what decides the root
//! instead.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag is an override, so it wins over an environment that could
    /// have answered. Both variables are set here deliberately: a resolver
    /// that checked the flag *after* `$XDG_CACHE_HOME` would still pass a
    /// test that left the environment empty.
    #[test]
    fn the_flag_wins_over_both_variables() {
        let resolved = cache_root(
            Some(Path::new("/srv/pinned")),
            Some(OsStr::new("/home/someone/.cache")),
            Some(OsStr::new("/home/someone")),
        );
        assert_eq!(resolved, Resolution::Flag(PathBuf::from("/srv/pinned")));
        assert_eq!(resolved.covers(), PathBuf::from("/srv/pinned/covers"));
    }

    /// `$XDG_CACHE_HOME` is the variable the specification says to consult
    /// first, and it is joined with the application name rather than used
    /// bare: it is *the cache directory*, shared with every other program.
    #[test]
    fn xdg_cache_home_wins_over_home() {
        let resolved = cache_root(
            None,
            Some(OsStr::new("/var/cache/someone")),
            Some(OsStr::new("/home/someone")),
        );
        assert_eq!(
            resolved,
            Resolution::Xdg(PathBuf::from("/var/cache/someone/taiyaku"))
        );
    }

    /// Without `$XDG_CACHE_HOME`, the specification's own default is
    /// `$HOME/.cache` — derived here rather than assumed to be exported.
    #[test]
    fn home_is_used_when_xdg_is_absent() {
        let resolved = cache_root(None, None, Some(OsStr::new("/home/someone")));
        assert_eq!(
            resolved,
            Resolution::Home(PathBuf::from("/home/someone/.cache/taiyaku"))
        );
    }

    /// Neither variable set is today's behaviour, kept as a fallback rather
    /// than a refusal: a cache is derived data, and a server that will not
    /// start because `$HOME` is unset trades a recoverable state for an
    /// unrecoverable one.
    #[test]
    fn neither_variable_falls_back_to_the_relative_path() {
        let resolved = cache_root(None, None, None);
        assert_eq!(resolved, Resolution::Fallback(PathBuf::from(".taiyaku-cache")));
        assert!(
            resolved.root().is_relative(),
            "the fallback is the relative path this module exists to stop being the default"
        );
    }

    /// An empty variable is unset, which the XDG specification states and
    /// which matters more here than it looks: `Path::new("").join("taiyaku")`
    /// is the *relative* `taiyaku`, so treating empty as set would silently
    /// reintroduce the exact defect this module removes — and without the
    /// `Fallback` variant, so nothing would print either.
    #[test]
    fn an_empty_variable_is_treated_as_unset() {
        let empty = cache_root(None, Some(OsStr::new("")), Some(OsStr::new("")));
        assert_eq!(empty, Resolution::Fallback(PathBuf::from(".taiyaku-cache")));

        let home_only = cache_root(None, Some(OsStr::new("")), Some(OsStr::new("/home/someone")));
        assert_eq!(
            home_only,
            Resolution::Home(PathBuf::from("/home/someone/.cache/taiyaku"))
        );
    }

    /// `covers` is a leaf *under* the root, not the root itself. If it
    /// returned the root, `ServeState::thumb_cache_dir`'s `with_file_name`
    /// would make the tile directory a sibling of the root rather than of the
    /// covers — putting tiles outside the application's own directory.
    #[test]
    fn covers_is_a_leaf_under_the_root() {
        let resolved = cache_root(None, Some(OsStr::new("/var/cache/someone")), None);
        assert_eq!(resolved.root(), Path::new("/var/cache/someone/taiyaku"));
        assert_eq!(
            resolved.covers(),
            PathBuf::from("/var/cache/someone/taiyaku/covers")
        );
        assert_eq!(resolved.covers().parent(), Some(resolved.root()));
    }
}
```

- [ ] **Step 2: Declare the module so the tests compile**

In `crates/taiyaku-serve/src/lib.rs`, add the module declaration immediately
after `pub mod atomic;` (alphabetical, before `mod composite;`):

```rust
mod cache_root;
mod composite;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-serve cache_root`

Expected: FAIL at compile time — `cannot find function 'cache_root' in this
scope`, and `cannot find type 'Resolution' in this scope`, once per test.

- [ ] **Step 4: Write the implementation**

Insert this into `crates/taiyaku-serve/src/cache_root.rs`, between the `use`
lines and `#[cfg(test)] mod tests`:

```rust
/// The directory this application takes under the shared platform cache root.
const APPLICATION: &str = "taiyaku";

/// The specification's own default when `$XDG_CACHE_HOME` is unset, joined
/// onto `$HOME`.
const XDG_DEFAULT: &str = ".cache";

/// What is used when neither variable can answer: the relative path this
/// module exists to stop being the *default*, kept as the last resort.
const RELATIVE_FALLBACK: &str = ".taiyaku-cache";

/// A cache root, and which rule produced it.
///
/// The rule is carried as a variant rather than the warning being printed
/// here, so the caller decides whether to say anything and a test can assert
/// which branch fired. Mutation testing has twice found survivors in this
/// workspace that were decisions living inside an `eprintln!` — `reqlog`'s
/// warn-once latch and `inventory`'s table loop, both recorded in
/// `docs/notes.md` — because no test of a component's *output* can see one.
/// Writing the fallback branch as a bare print would file a third by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// `--cache DIR` was given, and is used verbatim.
    Flag(PathBuf),
    /// `$XDG_CACHE_HOME` answered.
    Xdg(PathBuf),
    /// No `$XDG_CACHE_HOME`, but `$HOME` answered.
    Home(PathBuf),
    /// Neither variable answered. The one branch worth printing about,
    /// because it is the only one that still follows the working directory.
    Fallback(PathBuf),
}

impl Resolution {
    /// The root itself, whichever rule produced it.
    pub(crate) fn root(&self) -> &Path {
        match self {
            Self::Flag(p) | Self::Xdg(p) | Self::Home(p) | Self::Fallback(p) => p,
        }
    }

    /// Where generated covers go.
    ///
    /// `ServeState::thumb_cache_dir` derives `thumbs` from this by replacing
    /// the last component, so the two stay siblings under the root without
    /// this module naming `thumbs` at all — the arrangement that module's own
    /// doc comment argues for, on the grounds that only one of the two caches
    /// is cheap to rebuild.
    pub(crate) fn covers(&self) -> PathBuf {
        self.root().join("covers")
    }
}

/// Resolve the cache root from a flag and two environment variables.
///
/// Pure in its inputs: `serve` performs the `env::var_os` reads and passes
/// the results in. That is what lets every branch be tested without
/// `std::env::set_var`, which is `unsafe` in edition 2024 and racy under
/// nextest's threads besides — the alternative, a function that reads the
/// environment itself, is untestable on both counts.
///
/// `std::env::home_dir()` is deliberately not used. It was deprecated for
/// years and un-deprecated in 1.85, which is this workspace's exact
/// `rust-version`, and no 1.85 toolchain was available to check that against.
/// Reading the variables is what the XDG base directory specification says to
/// do regardless, so the question is avoided rather than answered.
pub(crate) fn cache_root(
    flag: Option<&Path>,
    xdg: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Resolution {
    if let Some(dir) = flag {
        return Resolution::Flag(dir.to_path_buf());
    }
    // Empty is unset, per the specification — and the reason matters here
    // more than the conformance does. `Path::new("").join("taiyaku")` is the
    // *relative* `taiyaku`, so an empty variable treated as set would put the
    // cache back under the working directory, which is the whole defect.
    if let Some(dir) = xdg.filter(|v| !v.is_empty()) {
        return Resolution::Xdg(Path::new(dir).join(APPLICATION));
    }
    if let Some(dir) = home.filter(|v| !v.is_empty()) {
        return Resolution::Home(Path::new(dir).join(XDG_DEFAULT).join(APPLICATION));
    }
    Resolution::Fallback(PathBuf::from(RELATIVE_FALLBACK))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve cache_root`

Expected: PASS, 6 tests. There will also be a `dead_code` warning for
`covers`, `root` and the variants — nothing calls them until Task 2. Do not
silence it with an `#[allow]`; Task 2 removes it.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/cache_root.rs crates/taiyaku-serve/src/lib.rs
git commit -m 'cache_root: resolve a root from a flag and two variables, not from the working directory'
```

---

### Task 2: Wire it into `serve()`

**Files:**
- Modify: `crates/taiyaku-serve/src/lib.rs:178-184` (the `serve` signature)
- Modify: `crates/taiyaku-serve/src/lib.rs:236-241` (the `ServeState` literal)
- Modify: `crates/taiyaku-cli/src/main.rs:1150` (the one call site, to keep it compiling)
- Test: `crates/taiyaku-serve/src/lib.rs`, the existing `#[cfg(test)] mod tests` that holds `the_tile_cache_is_a_sibling_of_the_cover_cache`

**Interfaces:**
- Consumes: `cache_root::cache_root`, `cache_root::Resolution` from Task 1.
- Produces: `pub async fn serve(works: Vec<Work>, addr: &str, stump: Option<StumpConfig>, log_requests: Option<PathBuf>, refresh_stump: bool, cache: Option<PathBuf>) -> Result<(), String>` — note `cache` is **last**, after the `bool`. See step 3.

- [ ] **Step 1: Write the failing test**

Add this to the same `#[cfg(test)] mod tests` in `lib.rs` that already holds
`the_tile_cache_is_a_sibling_of_the_cover_cache`, directly after it:

```rust
    /// The sibling invariant above, re-asserted at a *resolved* root rather
    /// than assumed to survive the move off the relative path. The two are
    /// different claims: that one holds `cache_dir` fixed from the fixture,
    /// this one takes whatever `cache_root` produced and checks the tiles
    /// still land beside the covers and inside the application's directory.
    #[test]
    fn a_resolved_root_keeps_covers_and_thumbs_as_siblings() {
        let mut state = crate::test_support::state("taiyaku-test-resolved-sibling");
        state.cache_dir = crate::cache_root::cache_root(
            None,
            Some(std::ffi::OsStr::new("/var/cache/fixture")),
            None,
        )
        .covers();
        let covers = state.cache_dir.clone();
        let thumbs = state.thumb_cache_dir();

        crate::test_support::cleanup("taiyaku-test-resolved-sibling");
        assert_eq!(
            covers,
            std::path::PathBuf::from("/var/cache/fixture/taiyaku/covers")
        );
        assert_eq!(
            thumbs,
            std::path::PathBuf::from("/var/cache/fixture/taiyaku/thumbs"),
            "tiles land beside the covers, inside the application's own directory"
        );
        assert_eq!(covers.parent(), thumbs.parent(), "siblings");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p taiyaku-serve a_resolved_root_keeps_covers`

Expected: FAIL at compile time — `field 'cache_dir' of struct 'ServeState' is
private`? No: it is `pub`. The real failure is that `state` returns an
immutable binding in the existing helper, so `let mut state` is fine and this
**compiles and passes** immediately. That is expected and correct: the test
pins the invariant Task 2's edit must not break, and it is written first so a
reviewer can see it was not written to match whatever the edit produced. If it
does not pass at this step, `cache_root` from Task 1 is wrong — stop and fix
that.

- [ ] **Step 3: Add the parameter to `serve`**

In `crates/taiyaku-serve/src/lib.rs`, change the signature at line 178:

```rust
pub async fn serve(
    works: Vec<Work>,
    addr: &str,
    stump: Option<StumpConfig>,
    log_requests: Option<std::path::PathBuf>,
    refresh_stump: bool,
    cache: Option<std::path::PathBuf>,
) -> Result<(), String> {
```

**`cache` goes last, after the `bool`, deliberately.** `log_requests` is also
an `Option<PathBuf>`; placing `cache` next to it would let the two be swapped
at the call site and still compile, turning a mistake into silent behaviour.
The `bool` between them makes any such swap a type error.

Six parameters is under `clippy::too_many_arguments`, which fires above seven,
so no config struct is warranted yet.

- [ ] **Step 4: Resolve the root and warn on the fallback**

In the same function, replace the `ServeState` literal at lines 236-241:

```rust
    // Resolved before the bind, so a person who is about to be surprised by
    // where their pages went reads it above `listening` rather than below it.
    let xdg = std::env::var_os("XDG_CACHE_HOME");
    let home = std::env::var_os("HOME");
    let resolved = cache_root::cache_root(cache.as_deref(), xdg.as_deref(), home.as_deref());
    if let cache_root::Resolution::Fallback(path) = &resolved {
        // The rule two hundred lines up, applied to a default rather than to
        // a flag: a thing that quietly does something surprising is worse
        // than one that says so. This branch is the only one that still
        // follows the working directory, which is the defect this module was
        // written to remove — so it is the one branch that must not be
        // silent.
        eprintln!(
            "taiyaku: neither XDG_CACHE_HOME nor HOME is set; caching under {}, which \
             follows the working directory. Pass --cache DIR to pin it.",
            path.display()
        );
    }

    let state = Arc::new(ServeState {
        works,
        source,
        cache_dir: resolved.covers(),
        log,
    });
```

- [ ] **Step 5: Update the one call site so the workspace compiles**

In `crates/taiyaku-cli/src/main.rs`, line 1150, pass `None` for now — Task 3
replaces it with the flag's value:

```rust
        .block_on(taiyaku_serve::serve(
            works,
            addr,
            stump,
            log,
            refresh_stump,
            None,
        ))
```

- [ ] **Step 6: Run the crate's tests to verify they pass**

Run: `cargo test -p taiyaku-serve`

Expected: PASS, including `a_resolved_root_keeps_covers_and_thumbs_as_siblings`
and the pre-existing `the_tile_cache_is_a_sibling_of_the_cover_cache`. The
`dead_code` warning from Task 1 is gone.

- [ ] **Step 7: Check the whole workspace still builds and lints**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: exit 0, no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-serve/src/lib.rs crates/taiyaku-cli/src/main.rs
git commit -m 'serve: take the cache root from cache_root rather than from the working directory'
```

---

### Task 3: `--cache DIR` on the CLI

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs:115-118` (the usage line)
- Modify: `crates/taiyaku-cli/src/main.rs:1093-1103` (`reject_unknown_flags`)
- Modify: `crates/taiyaku-cli/src/main.rs:1132` (beside the `--log-requests` read)
- Modify: `crates/taiyaku-cli/src/main.rs:1148-1151` (the call, replacing Task 2's `None`)
- Test: `crates/taiyaku-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `taiyaku_serve::serve`'s sixth parameter, `cache: Option<PathBuf>`, from Task 2.
- Produces: nothing other tasks depend on.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-cli/tests/cli.rs`:

```rust
/// `--cache` with nothing after it is a refusal rather than a silent fall
/// back to the default. The same contract every other value-taking flag in
/// this binary holds, and the reason `option_value` returns a `Result`.
///
/// Only refusal paths are exercised from here: a `serve` that parsed its
/// arguments successfully would bind a port and block forever.
#[test]
fn serve_refuses_cache_with_no_directory_after_it() {
    let run = taiyaku()
        .args(["serve", "some.json", "--cache"])
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("taiyaku: "),
        "the message names the tool: {stderr}"
    );
    assert!(
        stderr.contains("--cache needs a directory"),
        "and says what was missing: {stderr}"
    );
    assert!(
        run.stdout.is_empty(),
        "a refusal writes nothing to the output stream"
    );
}

/// Adding a flag to `serve`'s accepted list must not accept everything.
/// `reject_unknown_flags` is the guard, and a list edited by hand is exactly
/// where it could be widened by accident.
#[test]
fn serve_still_refuses_an_unknown_flag_and_lists_cache_among_the_real_ones() {
    let run = taiyaku()
        .args(["serve", "some.json", "--cash", "/tmp/x"])
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--cash"),
        "the refusal names what it refused: {stderr}"
    );
    assert!(
        stderr.contains("--cache"),
        "and the alphabet it listed includes the real flag: {stderr}"
    );
}

/// The usage names the flag. A flag that works but is not advertised is one
/// nobody finds, and this line is the only place `serve`'s options are
/// listed.
#[test]
fn the_usage_advertises_serve_cache() {
    let run = taiyaku().output().expect("the binary runs");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let serve_line = stdout
        .lines()
        .find(|l| l.contains("taiyaku serve"))
        .expect("the usage lists serve");
    assert!(
        serve_line.contains("[--cache DIR]"),
        "serve's usage line advertises the flag: {serve_line}"
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-cli --test cli`

Expected: FAIL. `serve_refuses_cache_with_no_directory_after_it` fails because
`--cache` is currently an *unknown* flag, so stderr says `serve has no flag
'--cache'` rather than `--cache needs a directory`.
`serve_still_refuses_an_unknown_flag_and_lists_cache_among_the_real_ones`
fails because the listed alphabet does not contain `--cache`.
`the_usage_advertises_serve_cache` fails because the usage line does not
contain `[--cache DIR]`.

- [ ] **Step 3: Accept the flag**

In `crates/taiyaku-cli/src/main.rs`, add `"--cache"` to the list at line 1096,
keeping it alphabetical:

```rust
    reject_unknown_flags(
        "serve",
        args,
        &[
            "--addr",
            "--cache",
            "--log-requests",
            "--refresh-stump",
            "--stump-key",
            "--stump-url",
        ],
    )?;
```

- [ ] **Step 4: Read its value and pass it through**

Immediately after the `let log = …` line (1132), add:

```rust
    // Read before `load_works` so `serve x.json --cache` with nothing after
    // it is refused for the reason that is actually wrong, rather than for a
    // sidecar that may also be missing.
    //
    // It names the *root*, and `taiyaku-serve` derives `covers` and `thumbs`
    // under it — the same convention `align --cache .taiyaku-cache` uses.
    // Naming the covers directory instead would put the tiles in a sibling of
    // whatever was typed rather than inside it.
    let cache = option_value(args, "--cache", "a directory")?.map(PathBuf::from);
```

Then replace Task 2's placeholder at the call:

```rust
        .block_on(taiyaku_serve::serve(
            works,
            addr,
            stump,
            log,
            refresh_stump,
            cache,
        ))
```

- [ ] **Step 5: Advertise it in the usage**

Replace lines 115-118:

```rust
            println!(
                "       taiyaku serve  <taiyaku.json>... [--addr HOST:PORT] [--stump-url URL] \
                 [--stump-key KEY] [--log-requests FILE] [--refresh-stump] [--cache DIR]"
            );
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli --test cli`

Expected: PASS, all tests in the file.

- [ ] **Step 7: Run the whole gate**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`

Expected: exit 0, no warnings, all tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-cli/src/main.rs crates/taiyaku-cli/tests/cli.rs
git commit -m 'serve: --cache DIR, closing the gap with the other four commands'
```

---

### Task 4: The six documentation sites

Every one of these is a sentence the change makes false. They are one task
because they are one claim stated in six places, and splitting them would
leave the repository self-contradictory between two commits.

The spec's §6 listed five. A sixth turned up when this plan's own step 6
verification was run against the tree before any code changed:
`docs/status.md:1273-1275` records the M5a tile cache at
`.taiyaku-cache/thumbs` as a present fact. It is corrected here.

**Files:**
- Modify: `docs/sidecar.md:70-73`
- Modify: `docs/status.md:206` (the `serve` usage line)
- Modify: `docs/status.md:1253-1275` (the O11 detail — three separate edits)
- Modify: `docs/status.md:3489-3499` (the "Later" entry)
- Modify: `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md:3` (the status line)

**Interfaces:** none. Documentation only.

- [ ] **Step 1: Correct `docs/sidecar.md`**

Replace the paragraph at lines 70-73 — currently *"`.taiyaku-cache/` is not
the fingerprint cache alone. `taiyaku serve` keeps a second cache beside it,
at `.taiyaku-cache/covers`…"* — with:

```markdown
`.taiyaku-cache/` is the fingerprint cache alone. `taiyaku serve` keeps its own
cache — one generated JPEG per edition for the OPDS feed, plus filmstrip tiles
and reading-size pages — under the platform cache root instead:
`$XDG_CACHE_HOME/taiyaku`, or `$HOME/.cache/taiyaku` where that is unset, with
`covers/` and `thumbs/` beneath it. `--cache DIR` overrides both. It used to
sit at `.taiyaku-cache/covers`, relative to wherever the server happened to be
started from, which meant a second cache and a regenerated volume for every
launch directory. See `status.md`'s note on the on-disk caches for what the
server's does and does not do (unbounded, never pruned).
```

- [ ] **Step 2: Correct the `serve` usage line in `docs/status.md`**

At line 206, inside the fenced command block, append `[--cache DIR]`:

```
taiyaku serve  <taiyaku.json>...                [--addr HOST:PORT] [--stump-url URL] [--stump-key KEY] [--log-requests FILE] [--refresh-stump] [--cache DIR]
```

- [ ] **Step 3: Correct the O11 detail**

Two edits in the paragraph at lines 1253-1269.

First, at line 1254-1257, *"covers live under `.taiyaku-cache/covers`, relative
to the server's working directory, and are unbounded and never pruned"*
becomes:

```markdown
  the same shape without the excuse of a design tradeoff behind it: covers are
  unbounded and never pruned — an archive that is replaced
```

(dropping the location clause, which the next edit restates correctly).

Second, replace the *"One thing worth knowing before anyone clears 'the
cache'"* passage at lines 1265-1269 with:

```markdown
  **The two caches no longer share a tree, and that is new.** They did:
  `.taiyaku-cache/covers/` was the server's, and the 55 `<sha256>.json` files
  beside it — about 1.2 MB — are the CLI's fingerprint cache, the one
  `--cache` names and the one measured at 3.4 s → 189 ms. Deleting the parent
  threw away both, and only one is cheap to rebuild. Since 2026-08-11 the
  server's cache lives under `$XDG_CACHE_HOME/taiyaku` (or
  `$HOME/.cache/taiyaku`), so `.taiyaku-cache/` holds the fingerprint cache
  alone and clearing it can no longer discard the expensive half by accident.
  The hazard is recorded rather than deleted because a warning that vanishes
  without explanation reads as an oversight.
```

Third, nine lines further down at 1273-1275, the M5a tile-cache measurement
names the old location as a present fact:

> **That second consumer now exists (M5a Task 2) and has been measured, in
> `.taiyaku-cache/thumbs`, a sibling of `.taiyaku-cache/covers` rather than the
> same directory.**

Replace those three lines with:

```markdown
  **That second consumer now exists (M5a Task 2) and has been measured, in a
  `thumbs` directory that is a sibling of `covers` rather than the same
  directory** — both under `.taiyaku-cache` when the measurement was taken,
  and both under the platform cache root since 2026-08-11; the sibling
  relationship is what the figures below depend on and it is unchanged.
  Fetching every tile of one 195-page edition
```

- [ ] **Step 4: Turn the "Later" entry into a shipped record**

Replace the entry at lines 3489-3499 — *"**The page cache follows the working
directory, and now that costs more.**"* — with:

```markdown
- **The page cache used to follow the working directory. Shipped 2026-08-11**
  on `serve-cache-root`; `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md`
  is the record, and this entry is kept as the defect that opened it.
  `ServeState`'s `cache_dir` defaulted to the *relative*
  `.taiyaku-cache/covers`, so where a cache landed depended on where `taiyaku
  serve` was launched from. That was cheap while the cache held covers; since
  `?w=` it holds every reading-size page, so starting the server from a
  different directory silently produced a second cache and regenerated a whole
  volume at ~126 ms a page. Found by noticing 120 MB of pages under `web/`
  after a session that had launched `serve` from there.
  It now resolves to `$XDG_CACHE_HOME/taiyaku`, falling back to
  `$HOME/.cache/taiyaku` and then — printing a line that says so — to the old
  relative path, with `--cache DIR` overriding all three. **Two alternatives
  the entry itself had proposed both lost.** Resolving against the sidecar,
  the way `stump::cache::path_for` does, cannot work as stated: that function
  takes a `&Work` and runs per work, while `cache_dir` is one directory for a
  server that may be handed sidecars in several directories, so there is no
  single sidecar to resolve against. Canonicalising against the first sidecar
  would have made the location depend on argument order instead, trading one
  surprise for another. Both would also have written into the corpus tree.
```

- [ ] **Step 5: Mark the spec as built**

In `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md`, line 3,
replace:

```markdown
**Status: settled in brainstorming on 2026-08-11. Not yet built.**
```

with:

```markdown
**Status: settled in brainstorming on 2026-08-11. Built, and shipped on
`serve-cache-root`.**
```

- [ ] **Step 6: Verify no stale claim survives**

Run:

```bash
grep -rn 'taiyaku-cache/covers\|taiyaku-cache/thumbs' crates scripts
```

Expected: **no matches at all.** A match means a code path or a gate still
names the old location.

`docs/` is deliberately not grepped: `docs/notes.md` and the plans under
`docs/superpowers/plans/` are dated working records that state what was true
when they were written and are not corrected retroactively, so they match by
design. The four live documentation sites are handled by steps 1-4 by hand.

- [ ] **Step 7: Run the full gate**

Run: `./scripts/check-all.sh`

Expected: exit 0. It mirrors all four CI jobs, so this is the last check
before the branch is pushed.

- [ ] **Step 8: Commit**

```bash
git add docs/
git commit -m 'docs: the served cache moved, and five sentences said otherwise'
```

---

## Self-Review

**Spec coverage.** Every section maps to a task: §1 (one field drives both) →
Task 2 step 4 and its test; §2 (where it lands, and the two alternatives) →
Task 1 step 4; §3 (precedence and the fallback warning) → Task 1 steps 1 and
4, Task 2 step 4; §4 (`--cache` naming the root) → Task 3; §5 (pure function,
variant not `eprintln!`) → Task 1 steps 1 and 4; §6 (doc sites) → Task 4,
which corrects **six** rather than the spec's five — `docs/status.md:1273-1275`
was missed by the spec and found by running this plan's own verification step
against the unmodified tree;
§7 (testing) → Task 1's six tests, Task 2's sibling test, Task 3's three CLI
tests; §8 (no migration, no pruning, CLI default untouched) → nothing is
built, and Task 4 step 6's grep confirms `crates/` no longer names the old
path; §9 (what could not be measured) → carried into `cache_root`'s doc
comment in Task 1 step 4.

**Spec §7 named seven tests; this plan writes ten.** The three extra are
`an_empty_variable_is_treated_as_unset` and `covers_is_a_leaf_under_the_root`
in Task 1, and `the_usage_advertises_serve_cache` in Task 3. The first is the
one that would otherwise let the defect back in silently; the other two pin
claims Task 2 and Task 3 depend on.

**Placeholders.** None. Every code step carries the code, every command
carries its expected output.

**Type consistency.** `cache_root(Option<&Path>, Option<&OsStr>, Option<&OsStr>)
-> Resolution` is defined in Task 1 and called in Task 2 step 4 with
`cache.as_deref()`, `xdg.as_deref()`, `home.as_deref()` — `Option<PathBuf>` and
`Option<OsString>` deref to exactly those. `Resolution::covers() -> PathBuf`
matches `ServeState::cache_dir: PathBuf`. `serve`'s sixth parameter is
`Option<PathBuf>` in Task 2 step 3 and is fed `option_value(...).map(PathBuf::from)`
in Task 3 step 4, which is `Option<PathBuf>`.

**One deliberate deviation from strict TDD**, flagged rather than hidden: Task
2 step 1's test passes as soon as it is written, because Task 1 already
supplies everything it asserts. It is written first anyway so a reviewer can
see the invariant was fixed before the edit rather than fitted to it. Step 2
says so explicitly and gives the stop condition if it does *not* pass.

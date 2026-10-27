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
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// `--cache DIR` was given, and is used verbatim.
    Flag(PathBuf),
    /// `$XDG_CACHE_HOME` answered.
    Xdg(PathBuf),
    /// No `$XDG_CACHE_HOME`, but `$HOME` answered.
    Home(PathBuf),
    /// Neither variable answered. The one branch worth printing about,
    /// because it is the only one that still follows the working directory:
    /// a relative variable is ignored rather than joined (see
    /// `from_variable`), and a relative `--cache` is a directory someone
    /// typed rather than a default that arrived unnoticed.
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
/// `std::env::home_dir()` is deliberately not used. Reading the variables is
/// what the XDG base directory specification says to do, whatever
/// `home_dir()`'s long deprecation history says about any given toolchain.
pub(crate) fn cache_root(
    flag: Option<&Path>,
    xdg: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Resolution {
    // An empty flag is an absent flag, and the argument for that is the one
    // `from_variable` makes below rather than a separate one: `--cache
    // "$SOMETHING_UNSET"` is a single unset shell variable away from an empty
    // argument, and `Path::new("").join("covers")` is the *relative* `covers`
    // — the defect this module exists to remove, reintroduced through the one
    // branch that prints nothing, since `Flag` is a rule that pins the root.
    // Falling through to the variables is what an absent flag already does.
    if let Some(dir) = flag.filter(|dir| !dir.as_os_str().is_empty()) {
        return Resolution::Flag(dir.to_path_buf());
    }
    if let Some(dir) = from_variable(xdg) {
        return Resolution::Xdg(dir.join(APPLICATION));
    }
    if let Some(dir) = from_variable(home) {
        return Resolution::Home(dir.join(XDG_DEFAULT).join(APPLICATION));
    }
    Resolution::Fallback(PathBuf::from(RELATIVE_FALLBACK))
}

/// A variable that may be used, or `None` for one the specification says to
/// ignore.
///
/// The XDG base directory specification: *"All paths set in these environment
/// variables must be absolute. If an implementation encounters a relative path
/// in any of these variables it should consider the path invalid and ignore
/// it."* Conformance is not the argument, though — the practical failure is.
/// `XDG_CACHE_HOME=.cache` run from `web/` caches into `web/.cache/taiyaku`
/// and run from the repository root into `./.cache/taiyaku`, which is the
/// working-directory-following defect back again, and silently, because `Xdg`
/// is one of the three rules `notice` treats as having pinned the root.
///
/// Empty is the degenerate case of relative rather than a second rule —
/// `Path::new("")` is not absolute — so `is_absolute` is one test where the
/// code previously had one test that caught only the degenerate half.
///
/// The flag is checked for emptiness but not for absoluteness: a relative
/// `--cache ./x` is a directory someone chose, where a relative variable is
/// one some unrelated program exported and nobody meant this to read.
fn from_variable(value: Option<&OsStr>) -> Option<&Path> {
    let path = Path::new(value?);
    path.is_absolute().then_some(path)
}

/// What to say about a resolution, or `None` for the three rules that pin the
/// root and so have nothing to warn about.
///
/// Returned rather than printed for exactly the reason the `Resolution` doc
/// comment above gives. Carrying the rule as a variant only moves the decision
/// as far as the caller: `serve` still had to choose which variant is worth a
/// line, and that choice lived inside an `eprintln!` where no test could see
/// it — a mutant deleting the whole branch survived the suite, the third of
/// the kind `reqlog` and `inventory` already filed. As a value, both halves
/// are assertable: that `Fallback` speaks, and that the other three do not.
pub(crate) fn notice(resolved: &Resolution) -> Option<String> {
    match resolved {
        Resolution::Fallback(path) => Some(format!(
            "taiyaku: neither XDG_CACHE_HOME nor HOME is set; caching under {}, which \
             follows the working directory. Pass --cache DIR to pin it.",
            path.display()
        )),
        Resolution::Flag(_) | Resolution::Xdg(_) | Resolution::Home(_) => None,
    }
}

/// Create the resolved root, or say what could not be created.
///
/// Nothing used to create it at all. The first `create_dir_all` happened
/// lazily inside `cover.rs`, where a failure is deliberately discarded so that
/// an unwritable cache is slow rather than broken — right per request, and
/// wrong at startup. `--cache /etc/hosts` names an existing *file*; the server
/// starts, prints `listening`, serves every request correctly, and regenerates
/// every cover, tile and reading-size page at 126 ms a page forever, with
/// nothing said once. A directory under a read-only mount does the same.
///
/// This repository has filed that exact shape before: O22 came out of
/// measuring that `verify a -- b --cache DIR` did not create DIR (see
/// `taiyaku-cli/src/main.rs`'s `verify`).
///
/// **A complaint, not a refusal**, for the reason the design's §3 gives about
/// the unset-variable case: a cache is derived data, and a server that will
/// not start because a cache directory cannot be made trades a recoverable
/// state for an unrecoverable one. Returned rather than printed for `notice`'s
/// reason, which is the same reason — a decision inside an `eprintln!` is one
/// no test of this module's output can see.
///
/// `covers` rather than the bare root, because the bare root is the weaker
/// check: `create_dir_all` succeeds on a directory that already exists, so a
/// root sitting on a read-only mount — one of the two shapes this exists to
/// catch — would pass it. Creating a child proves the root is writable, and
/// `thumbs` is created beside it later by the same right.
pub(crate) fn create(resolved: &Resolution) -> Option<String> {
    let covers = resolved.covers();
    std::fs::create_dir_all(&covers).err().map(|e| {
        format!(
            "taiyaku: could not create the cache directory {} ({e}); covers, tiles and \
             reading-size pages will be regenerated on every request rather than kept. \
             Pass --cache DIR to name a directory that can be created.",
            covers.display()
        )
    })
}

#[cfg(test)]
mod tests;

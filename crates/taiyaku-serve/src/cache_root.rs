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
        assert_eq!(
            resolved,
            Resolution::Fallback(PathBuf::from(".taiyaku-cache"))
        );
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

        let home_only = cache_root(
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("/home/someone")),
        );
        assert_eq!(
            home_only,
            Resolution::Home(PathBuf::from("/home/someone/.cache/taiyaku"))
        );
    }

    /// Important 1 (whole-branch review): an *empty* flag is an absent flag.
    ///
    /// `--cache "$SOMETHING_UNSET"` arrives here as `Some("")`, and taken
    /// verbatim it produced `Flag("")` — whose `covers()` is the relative
    /// `covers` and whose tiles land in a relative `thumbs`, which is the
    /// defect this module removes, through the one branch `notice` keeps
    /// silent because it is supposed to have pinned the root. Worse than what
    /// was there before: `.gitignore` ignores `.taiyaku-cache/` and neither of
    /// those two names.
    ///
    /// The variables are set here, so the assertion is that resolution falls
    /// *through* to them rather than merely that it stopped producing `Flag`.
    #[test]
    fn an_empty_flag_is_treated_as_absent() {
        let resolved = cache_root(
            Some(Path::new("")),
            Some(OsStr::new("/var/cache/someone")),
            Some(OsStr::new("/home/someone")),
        );

        assert_eq!(
            resolved,
            Resolution::Xdg(PathBuf::from("/var/cache/someone/taiyaku")),
            "an empty flag must fall through to the variables, not pin the relative root"
        );
        assert!(
            resolved.covers().is_absolute(),
            "the failure this guards is a relative `covers` in the working directory"
        );
    }

    /// Important 3 (whole-branch review): a *relative* variable is invalid and
    /// ignored, which the XDG base directory specification states outright —
    /// "if an implementation encounters a relative path in any of these
    /// variables it should consider the path invalid and ignore it".
    ///
    /// The conformance is the smaller half. `XDG_CACHE_HOME=.cache` run from
    /// `web/` cached into `web/.cache/taiyaku`, following the working
    /// directory exactly as the unresolved `cache_dir` did — and silently,
    /// since `Xdg` is a rule `notice` treats as having pinned the root.
    ///
    /// Both variables are exercised: a resolver that filtered only
    /// `$XDG_CACHE_HOME` would pass a test that checked one.
    #[test]
    fn a_relative_variable_is_ignored() {
        let relative_xdg = cache_root(
            None,
            Some(OsStr::new(".cache")),
            Some(OsStr::new("/home/someone")),
        );
        assert_eq!(
            relative_xdg,
            Resolution::Home(PathBuf::from("/home/someone/.cache/taiyaku")),
            "a relative $XDG_CACHE_HOME is invalid, so $HOME answers instead"
        );

        let both_relative = cache_root(
            None,
            Some(OsStr::new(".cache")),
            Some(OsStr::new("relative/home")),
        );
        assert_eq!(
            both_relative,
            Resolution::Fallback(PathBuf::from(RELATIVE_FALLBACK)),
            "with neither variable usable it is the fallback — which, unlike Xdg or Home, \
             says so on stderr"
        );
        assert!(
            notice(&both_relative).is_some(),
            "and that is the whole point: the branch that follows the working directory \
             is the branch that speaks"
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

    /// The three rules that pin the root say nothing. All three are asserted
    /// rather than one taken as representative: `notice` is a match, and a
    /// test that checked only `Xdg` would pass with the `Flag` and `Home` arms
    /// both mutated into a message.
    #[test]
    fn the_pinned_rules_are_silent() {
        for resolved in [
            Resolution::Flag(PathBuf::from("/srv/pinned")),
            Resolution::Xdg(PathBuf::from("/var/cache/someone/taiyaku")),
            Resolution::Home(PathBuf::from("/home/someone/.cache/taiyaku")),
        ] {
            assert_eq!(
                notice(&resolved),
                None,
                "{resolved:?} pins the root, so there is nothing to warn about"
            );
        }
    }

    /// The fallback speaks, and says the two things a person needs: where the
    /// pages actually went, and how to stop it happening again.
    ///
    /// Asserted by content rather than against a copy of the literal, which
    /// would check only that two copies of the wording match and would have to
    /// be edited every time the wording improved.
    #[test]
    fn the_fallback_names_the_path_and_the_flag() {
        let resolved = cache_root(None, None, None);
        let notice = notice(&resolved).expect("the fallback is the branch that must not be silent");
        assert!(
            notice.contains(RELATIVE_FALLBACK),
            "the notice must name the root it resolved to, not just describe one: {notice}"
        );
        assert!(
            notice.contains("--cache"),
            "the notice must say how to pin the root: {notice}"
        );
    }

    /// A directory named for a test of `create`, under the temp dir and never
    /// under a real `$HOME` — a test that writes where the person running it
    /// keeps their own cache is one that passes on this machine and says
    /// nothing about any other. Named per test and per process, the
    /// convention `cover.rs`'s `tempdir_for_test` set, so two tests running in
    /// parallel cannot delete each other's fixture.
    fn scratch(what: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "taiyaku-test-cache-root-{what}-{}",
            std::process::id()
        ))
    }

    /// Important 5 (whole-branch review): the root exists once `serve` has
    /// started, rather than the first time a request happens to want it.
    ///
    /// Asserted on the directory rather than only on the returned `None`,
    /// since a `create` that reported success and made nothing would leave
    /// every generation writing into a directory that is not there.
    #[test]
    fn creating_the_root_makes_it_and_says_nothing() {
        let root = scratch("created");
        let _ = std::fs::remove_dir_all(&root);
        let resolved = Resolution::Flag(root.clone());

        let complaint = create(&resolved);
        let made = resolved.covers().is_dir();
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(complaint, None, "a root that can be created says nothing");
        assert!(
            made,
            "and it exists before the first request, not after one"
        );
    }

    /// A root that cannot be created is said out loud and is *not* fatal.
    ///
    /// Staged as an existing file where a directory has to go — `--cache
    /// /etc/hosts` reduced to something a test may create — which is the shape
    /// that used to start the server, print `listening`, and then regenerate
    /// every page on every request at 126 ms apiece with nothing ever said.
    ///
    /// Both halves are asserted: that there is a complaint at all, and that it
    /// names the path and the flag, since a line reading "the cache could not
    /// be created" without either is one a person cannot act on.
    #[test]
    fn a_root_that_cannot_be_created_is_reported_rather_than_fatal() {
        let file = scratch("is-a-file");
        let _ = std::fs::remove_dir_all(&file);
        std::fs::write(&file, b"an existing file where a directory has to go").unwrap();

        let complaint = create(&Resolution::Flag(file.clone()));
        let _ = std::fs::remove_file(&file);

        let complaint = complaint.expect("a cache root that cannot be created must not be silent");
        assert!(
            complaint.contains(&file.display().to_string()),
            "the complaint must name the directory it could not create: {complaint}"
        );
        assert!(
            complaint.contains("--cache"),
            "and how to name one that works: {complaint}"
        );
    }
}

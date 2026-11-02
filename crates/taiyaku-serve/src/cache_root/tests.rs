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

/// Neither variable set falls back to the relative path rather than refusing:
/// a cache is derived data, and a server that will not start because `$HOME`
/// is unset trades a recoverable state for an unrecoverable one.
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

/// An *empty* flag is an absent flag: `--cache "$SOMETHING_UNSET"` arrives
/// here as `Some("")`.
///
/// The variables are set here, so the assertion is that resolution falls
/// *through* to them rather than merely that it does not produce `Flag`.
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

/// A *relative* variable is invalid and ignored, which the XDG base directory
/// specification states outright — "if an implementation encounters a relative
/// path in any of these variables it should consider the path invalid and
/// ignore it".
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

/// The root exists once `serve` has started, rather than the first time a
/// request happens to want it.
///
/// Asserted on the directory rather than only on the returned `None`,
/// since a `create` that reported success and made nothing would leave
/// every generation writing into a directory that is not there.
#[test]
fn creating_the_root_makes_it_and_says_nothing() {
    // Under the temp dir and never under a real `$HOME`: a test that writes
    // where the person running it keeps their own cache passes on this
    // machine and says nothing about any other.
    let scratch = crate::test_support::Fixture::new("cache-root-created");
    let root = scratch.join("root");
    let resolved = Resolution::Flag(root.clone());

    let complaint = create(&resolved);
    let made = resolved.covers().is_dir();

    assert_eq!(complaint, None, "a root that can be created says nothing");
    assert!(
        made,
        "and it exists before the first request, not after one"
    );
}

/// A root that cannot be created is said out loud and is *not* fatal.
///
/// Staged as an existing file where a directory has to go, which is what
/// `--cache /etc/hosts` names, reduced to something a test may create.
///
/// Both halves are asserted: that there is a complaint at all, and that it
/// names the path and the flag, since a line reading "the cache could not
/// be created" without either is one a person cannot act on.
#[test]
fn a_root_that_cannot_be_created_is_reported_rather_than_fatal() {
    let scratch = crate::test_support::Fixture::new("cache-root-is-a-file");
    let file = scratch.join("is-a-file");
    std::fs::write(&file, b"an existing file where a directory has to go").unwrap();

    let complaint = create(&Resolution::Flag(file.clone()));

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

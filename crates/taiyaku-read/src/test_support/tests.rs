use super::TempDir;
use std::path::Path;

/// Two directories asked for under one label are two directories, and both
/// exist. Under `cargo test` every test in a crate shares one process, so the
/// counter is what keeps two of them apart.
#[test]
fn two_temp_dirs_under_one_label_are_distinct_and_both_exist() {
    let first = TempDir::new("same-label");
    let second = TempDir::new("same-label");

    assert_ne!(first.path(), second.path());
    assert!(
        first.path().is_dir(),
        "{} was not created",
        first.path().display()
    );
    assert!(
        second.path().is_dir(),
        "{} was not created",
        second.path().display()
    );
}

/// Dropping one removes it, contents and all. A unique name without this is
/// one leaked directory per test per run.
#[test]
fn a_temp_dir_is_removed_with_its_contents_when_dropped() {
    let dir = TempDir::new("dropped");
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested").join("file"), b"x").unwrap();
    let path = dir.path().to_path_buf();

    drop(dir);

    assert!(!path.exists(), "{} outlived its TempDir", path.display());
}

/// The name carries this process's id. A counter alone restarts at nought in
/// every process — every test, under `cargo nextest` — so two checkouts
/// running at once would hand out the same names, which is O117.
#[test]
fn a_temp_dir_is_named_for_the_process_that_made_it() {
    let dir = TempDir::new("pid");
    let name = dir.path().file_name().unwrap().to_str().unwrap();
    let pid = std::process::id().to_string();

    assert!(
        name.split('-').any(|part| part == pid),
        "{name} does not carry this process's id, {pid}"
    );
    assert!(
        dir.path().starts_with(std::env::temp_dir()),
        "{} is not under the temp dir, so TMPDIR cannot move it",
        dir.path().display()
    );
}

/// **O117's standing check: no file under `crates/` but this helper calls
/// `temp_dir()` itself.** A new test that names its own path there instead of
/// using [`TempDir`] fails here, naming its line, rather than failing some
/// later parallel run for a reason nobody can see.
///
/// Comment lines are skipped. No shipped code calls `temp_dir()` today; if
/// some ever must, exempt its file here by name, with the reason.
#[test]
fn nothing_outside_this_helper_calls_temp_dir() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("taiyaku-read sits under crates/");
    let helper_file = crates.join("taiyaku-read/src/test_support.rs");
    let helper_tests = crates.join("taiyaku-read/src/test_support");
    // Split so this file's own source does not hold the text it looks for.
    let needle = concat!("temp_", "dir()");

    let mut scanned = 0;
    let mut found = Vec::new();
    let mut pending = vec![crates.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "target") {
                    pending.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs")
                || path == helper_file
                || path.starts_with(&helper_tests)
            {
                continue;
            }
            scanned += 1;
            let source = std::fs::read_to_string(&path).unwrap();
            for (index, line) in source.lines().enumerate() {
                if !line.trim_start().starts_with("//") && line.contains(needle) {
                    found.push(format!(
                        "{}:{}",
                        path.strip_prefix(crates).unwrap().display(),
                        index + 1
                    ));
                }
            }
        }
    }

    assert!(
        scanned > 100,
        "the walk read {scanned} files under {}, so it did not reach the workspace's sources",
        crates.display()
    );
    assert!(
        found.is_empty(),
        "use taiyaku_read::test_support::TempDir instead of naming a path under the temp dir: {found:#?}"
    );
}

/// A label is one path component. `a/b` would make `taiyaku-test-a` and leave
/// it behind when only `b-{pid}-{n}` beneath it is removed.
#[test]
#[should_panic(expected = "a TempDir label is one path component")]
fn a_label_holding_a_slash_is_refused() {
    let _ = TempDir::new("a/b");
}

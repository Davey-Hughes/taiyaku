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
    // A source tree unpacked inside another repository must not report that
    // repository's HEAD, nor rerun on its state.
    if top_level(&root).as_deref() != Some(root.as_path()) {
        return;
    }
    // Watched whether or not git gives facts below: a HEAD with no commit yet
    // gives none, and its first commit must run this again.
    for path in git_state(&root) {
        watch(&path);
    }
    if let Some(facts) = git_facts(&root) {
        println!("cargo:rustc-env=TAIYAKU_GIT_HEAD={}", facts.head);
        if facts.dirty {
            println!("cargo:rustc-env=TAIYAKU_GIT_DIRTY=1");
        }
    }
}

/// The top of the repository `root` is in, or `None` where git is missing,
/// refuses the repository or finds none.
fn top_level(root: &Path) -> Option<PathBuf> {
    PathBuf::from(git(root, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .ok()
}

struct Facts {
    head: String,
    dirty: bool,
}

/// Every fact or none: a commit reported without its mark would be a wrong
/// label, and no label is better than a wrong one.
fn git_facts(root: &Path) -> Option<Facts> {
    let head = git(root, &["rev-parse", "HEAD"])?;
    let dirty = !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    Some(Facts { head, dirty })
}

/// The files git changes when HEAD moves or the index does.
fn git_state(root: &Path) -> Vec<PathBuf> {
    let (Some(git_dir), Some(common)) = (
        git(root, &["rev-parse", "--git-dir"]),
        git(root, &["rev-parse", "--git-common-dir"]),
    ) else {
        return Vec::new();
    };
    let git_dir = within(root, git_dir);
    let common = within(root, common);
    let mut watch = vec![
        git_dir.join("HEAD"),
        git_dir.join("index"),
        // Every move of HEAD appends to this log, whatever else it writes.
        git_dir.join("logs/HEAD"),
        common.join("packed-refs"),
        // The `reftable` format keeps refs and their logs in tables listed
        // here, per worktree and shared, in place of the ref and log files
        // above, so a move of HEAD shows here.
        git_dir.join("reftable/tables.list"),
        common.join("reftable/tables.list"),
    ];
    // Detached, `symbolic-ref` fails and there is no branch ref to watch.
    if let Some(branch) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
        watch.extend(branch_ref(&common, &branch));
    }
    // Outside a linked worktree the git dir is the common dir.
    watch.sort();
    watch.dedup();
    watch
}

/// The branch's loose ref, or where it has none, the nearest directory under
/// `refs/heads` that will hold it.
///
/// A branch with no commit yet, or whose ref is only in `packed-refs`, has no
/// file of its own. Its next commit writes one there and need not change
/// anything else watched here: a first commit has no reflog before it, and
/// where reflogs are off git starts none. Cargo scans a watched directory
/// whole, so a commit on any branch under it, a sibling worktree's included,
/// runs this again too. Never `refs/` or the git dir whole: every fetch would
/// run this again, and every build would rescan the objects.
fn branch_ref(common: &Path, branch: &str) -> Option<PathBuf> {
    let loose = common.join(branch);
    if loose.exists() {
        return Some(loose);
    }
    let heads = common.join("refs/heads");
    loose
        .ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(&heads))
        .find(|dir| dir.is_dir())
        .map(Path::to_path_buf)
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

/// git answers `--git-dir` and `--git-common-dir` relative to `root` where it
/// can.
fn within(root: &Path, answer: String) -> PathBuf {
    let path = PathBuf::from(answer);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
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

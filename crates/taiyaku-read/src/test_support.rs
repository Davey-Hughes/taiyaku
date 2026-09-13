//! Helpers for tests in this crate and, through the `test-support` feature,
//! in crates that depend on it.
//!
//! **Why a feature rather than a copy in each crate.** O104's strace harness
//! is needed by `atomic`'s tests here and by `stamp`'s in `taiyaku-library`,
//! and a second copy of a log parser is a second place for the two to drift
//! apart — one of them silently matching nothing. `taiyaku-library` enables
//! the feature only under `[dev-dependencies]`, so it is compiled into that
//! crate's test builds and any `--all-features` build, and into no binary
//! this workspace ships. The file name is the one the coverage report and
//! `.cargo/mutants.toml` already exclude, so neither gate needed changing.

use std::path::{Path, PathBuf};

/// One call from a strace log, reduced to what O104's ordering needs.
#[derive(Debug)]
pub enum Call {
    /// A successful write of any kind — `write`, `pwrite64`, `writev`,
    /// `copy_file_range` and the rest — to the files at these paths.
    Write(Vec<PathBuf>),
    /// An `fsync` or `fdatasync` of the file or directory at this path.
    Sync(PathBuf),
    /// A `rename`, `renameat` or `renameat2` of `from` onto `to`.
    Rename { from: PathBuf, to: PathBuf },
}

/// Every syscall that can put bytes into a file, so a write strace did not
/// report cannot hide after a sync.
const WRITES: &[&str] = &[
    "write",
    "writev",
    "pwrite64",
    "pwritev",
    "pwritev2",
    "copy_file_range",
    "sendfile",
    "splice",
    "ftruncate",
    "fallocate",
];

/// Of [`WRITES`], the ones whose arguments are all descriptors and numbers —
/// no data string — so every path in the line is one the call touched.
const DESCRIPTORS_ONLY: &[&str] = &["copy_file_range", "sendfile", "splice"];

/// Run one test of this binary again as a child under `strace`, and return
/// every successful write, sync and rename it made, in order.
///
/// The child is this same test binary, asked for exactly `test` and handed
/// `child` through the environment; the test notices the variable and does
/// only the traced work. `-y` has strace print the path behind every file
/// descriptor, so a call names the file it touched without this parser
/// tracking `openat` and `close`.
///
/// **A missing `strace` fails the test rather than skipping it**, unlike the
/// `NOT MEASURED:` shape `format/tests.rs` uses for `tar`. That test checks
/// that a fixture is well formed; this one is the only thing standing
/// between a later edit and a crash destroying a reader's file, and a skip
/// is a green run that measured nothing.
///
/// # Panics
///
/// If `strace` is not on `PATH`, if the traced child fails, or if strace
/// wrote no log.
#[must_use]
pub fn traced(test: &str, child: (&str, &std::ffi::OsStr), log: &Path) -> Vec<Call> {
    let exe = std::env::current_exe().expect("the test binary's own path");
    let ran = std::process::Command::new("strace")
        .args(["-f", "-qq", "-y", "-e"])
        .arg(format!(
            "trace=fsync,fdatasync,rename,renameat,renameat2,{}",
            WRITES.join(",")
        ))
        .arg("-o")
        .arg(log)
        .arg(exe)
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(child.0, child.1)
        .output()
        .expect("`strace` must be on PATH: O104's sync ordering cannot be observed without it");
    assert!(
        ran.status.success(),
        "the traced child failed: {}\n{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let recorded = std::fs::read_to_string(log).expect("strace wrote its log");
    rejoined(&recorded)
        .iter()
        .filter_map(|line| call(line))
        .collect()
}

/// strace's log with every interrupted call put back on one line.
///
/// Under `-f`, a call still in progress when another thread's traced call
/// starts is printed as `1234 write(3</a>, "x", 1 <unfinished ...>` and later
/// finished as `1234 <... write resumed>) = 1`. Tracing writes makes that
/// likelier than tracing syncs alone did, and either half on its own parses
/// as nothing — a write that silently vanished from the order.
fn rejoined(recorded: &str) -> Vec<String> {
    let mut pending: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    let mut lines = Vec::new();
    for line in recorded.lines() {
        let pid = line.split_whitespace().next().unwrap_or_default();
        if let Some(head) = line.strip_suffix(" <unfinished ...>") {
            pending.insert(pid, head.to_string());
        } else if let Some(at) = line.find(" resumed>") {
            let head = pending.remove(pid).unwrap_or_default();
            lines.push(format!("{head}{}", &line[at + " resumed>".len()..]));
        } else {
            lines.push(line.to_string());
        }
    }
    lines
}

/// One line of the log as a [`Call`], or nothing for a call that failed or is
/// not one of the three kinds. A call succeeded when what follows its last
/// `) = ` is a non-negative count.
fn call(line: &str) -> Option<Call> {
    let (_, result) = line.rsplit_once(") = ")?;
    result.split_whitespace().next()?.parse::<u64>().ok()?;

    let name = line.split_whitespace().nth(1)?.split('(').next()?;
    let args = &line[line.find('(')? + 1..];
    if name == "fsync" || name == "fdatasync" {
        return Some(Call::Sync(decorated(args).next()?));
    }
    if name.starts_with("rename") {
        let mut quoted = args.split('"').skip(1).step_by(2).map(PathBuf::from);
        return Some(Call::Rename {
            from: quoted.next()?,
            to: quoted.next()?,
        });
    }
    if WRITES.contains(&name) {
        // A data string can hold `<` and `>` of its own — a ComicInfo
        // document is XML — so outside the descriptor-only calls only the
        // first decoration, the destination descriptor, is trusted.
        let paths: Vec<PathBuf> = if DESCRIPTORS_ONLY.contains(&name) {
            decorated(args).collect()
        } else {
            decorated(args).take(1).collect()
        };
        return Some(Call::Write(paths));
    }
    None
}

/// The paths `-y` printed after descriptors in `args`, in order.
fn decorated(args: &str) -> impl Iterator<Item = PathBuf> + '_ {
    args.split('<').skip(1).filter_map(|part| {
        let (path, _) = part.split_once('>')?;
        Some(PathBuf::from(path))
    })
}

/// O104's ordering for a staged file renamed onto `path` inside `dir`: every
/// write to the staged file, then a sync of it, then the rename, then a sync
/// of `dir` — and nothing written to `path` after the rename.
///
/// **A sync before the last write proves nothing**, which is what the first
/// version of these tests allowed: syncing the empty temporary and then
/// writing to it, or syncing a stamp's staged archive before `finish` wrote
/// its central directory, both passed. So the sync must follow the last
/// write, and at least one write must have been seen at all, or an
/// unrecognised write syscall would make the whole ordering vacuous.
///
/// # Panics
///
/// Naming whichever step is missing or out of order.
pub fn assert_replaced_durably(calls: &[Call], path: &Path, dir: &Path) {
    let renamed = calls
        .iter()
        .position(|c| matches!(c, Call::Rename { to, .. } if to == path))
        .unwrap_or_else(|| panic!("nothing was renamed onto {}: {calls:?}", path.display()));
    let Call::Rename { from: staged, .. } = &calls[renamed] else {
        unreachable!()
    };
    let (before, after) = (&calls[..renamed], &calls[renamed + 1..]);

    let last_write = before.iter().rposition(|c| wrote_to(c, staged)).unwrap_or_else(|| {
        panic!(
            "no write to {} was seen before the rename, so its ordering proves nothing: {calls:?}",
            staged.display()
        )
    });
    assert!(
        before[last_write + 1..]
            .iter()
            .any(|c| matches!(c, Call::Sync(p) if p == staged)),
        "{} was renamed onto {} without being synced after its last write: {calls:?}",
        staged.display(),
        path.display()
    );
    assert!(
        !after.iter().any(|c| wrote_to(c, path)),
        "{} was written to after the rename, where no sync before it covers the bytes: {calls:?}",
        path.display()
    );
    assert!(
        after.iter().any(|c| matches!(c, Call::Sync(p) if p == dir)),
        "{} was not synced after the rename, so the rename may not survive a crash: {calls:?}",
        dir.display()
    );
}

/// Whether `call` wrote to the file at `target`.
fn wrote_to(call: &Call, target: &Path) -> bool {
    matches!(call, Call::Write(paths) if paths.iter().any(|p| p == target))
}

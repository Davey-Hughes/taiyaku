//! Which Stump book each member is, remembered beside the sidecar.
//!
//! Keyed by the member's `sha256` rather than its path: two members with the
//! same hash are the same file, and a work that names one archive twice should
//! resolve it once.
//!
//! **Absence is recorded too.** Most members of most works in this library are
//! not in Stump at all — every raw scan, every fan translation — and a miss
//! that is not remembered is a GraphQL round trip on every page turn.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::works::Work;

/// Member `sha256` → the Stump book it is, or `None` for "Stump has not got it".
pub type Resolutions = HashMap<String, Option<String>>;

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    taiyaku_stump: u32,
    /// The instance these answers came from.
    ///
    /// Recorded so a cache written against one Stump is not silently believed
    /// against another, where the same ids mean different books or nothing.
    base: String,
    members: Resolutions,
}

/// Where a work's Stump mapping sits: beside its sidecar, same stem.
///
/// The convention M2 established for progress. `taiyaku.json` stays a pure
/// alignment document that diffs meaningfully in git.
pub fn path_for(work: &Work) -> PathBuf {
    let stem = work
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "taiyaku".to_string());
    work.path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!("{stem}.stump.json"))
}

/// What was resolved against this instance, or nothing.
///
/// Every failure is "nothing", deliberately: the cache is derived data that
/// can be rebuilt in one round trip per member, so refusing to start over a
/// truncated file would trade a recoverable state for an unrecoverable one.
pub fn load(work: &Work, base: &str) -> Resolutions {
    let Ok(text) = std::fs::read_to_string(path_for(work)) else {
        return Resolutions::new();
    };
    let Ok(document) = serde_json::from_str::<Document>(&text) else {
        return Resolutions::new();
    };
    if document.taiyaku_stump != 1 || document.base != base {
        return Resolutions::new();
    }
    document.members
}

/// Write the mapping, atomically.
///
/// Write-and-rename rather than `fs::write`, which truncates first: a crash
/// mid-write would otherwise leave a short file. The same defect is recorded
/// against progress in `status.md`; this one does not ship with it.
pub fn store(work: &Work, base: &str, members: &Resolutions) -> Result<(), String> {
    let document = Document {
        taiyaku_stump: 1,
        base: base.to_string(),
        members: members.clone(),
    };
    let text = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    let final_path = path_for(work);
    let temporary = final_path.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|e| format!("{}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, &final_path).map_err(|e| format!("{}: {e}", final_path.display()))
}

/// Throw away what this work resolved to, so the next start asks again.
///
/// O11's escape hatch. Nothing watches for the library changing on Stump's
/// side — a deliberate tradeoff, recorded as Risk 5 in the design, because a
/// remembered miss costs one query once instead of one per page turn — so a
/// book added to Stump after a work first resolved stays invisible until this
/// file goes away. That tradeoff stands. What did not stand is that acting on
/// it required knowing from the source that a file called `taiyaku.stump.json`
/// exists and may be deleted.
///
/// Destructive in exactly the way deleting it by hand is, and for the same
/// reason it is safe: these are derived answers, rebuildable in one query per
/// member.
///
/// A work with nothing recorded is not an error. `--refresh-stump` is typed
/// once and applies to every work the server was handed, most of which will
/// have no cache file; refusing to start because there was nothing to delete
/// would be the worse answer.
pub fn forget(work: &Work) -> Result<(), String> {
    let path = path_for(work);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(dir: &str) -> Work {
        crate::test_support::work(dir)
    }

    /// O11: nothing watches for the library changing on Stump's side, so a
    /// book added there after a work's first resolution stays invisible until
    /// this file goes away. That was true before and stays true — it is the
    /// deliberate tradeoff the design records as Risk 5, and the reason for it
    /// (a miss costing one query once rather than one per page turn) has not
    /// changed. What was wrong is that the only way to act on it was to know,
    /// from reading the source, that a file called `taiyaku.stump.json` exists
    /// and can be deleted.
    ///
    /// So this is the manual workaround made into an operation. Deliberately
    /// destructive in exactly the way deleting the file by hand is: the
    /// answers are derived data, rebuildable in one query per member, and the
    /// whole point is to stop believing them.
    #[test]
    fn forgetting_a_work_makes_the_next_run_ask_stump_again() {
        let work = work("taiyaku-test-stump-cache-forget");
        let mut resolutions = Resolutions::new();
        resolutions.insert("hash-of-en-mono".into(), Some("a8ed51d8".into()));
        store(&work, "http://nas:10801", &resolutions).unwrap();

        let before = load(&work, "http://nas:10801");
        forget(&work).unwrap();
        let after = load(&work, "http://nas:10801");
        let gone = !path_for(&work).exists();

        crate::test_support::cleanup("taiyaku-test-stump-cache-forget");
        assert_eq!(before.len(), 1, "the resolution was there to forget");
        assert!(after.is_empty(), "and is not there afterwards: {after:?}");
        assert!(gone, "the file itself is removed, not emptied");
    }

    /// Forgetting a work that never resolved anything is not an error. The
    /// flag is a thing a person types before starting a server, across every
    /// work it was handed, and most of them will not have a cache file — one
    /// refusing to start the server because there was nothing to delete would
    /// be a worse answer than doing nothing.
    #[test]
    fn forgetting_a_work_with_nothing_recorded_is_not_an_error() {
        let work = work("taiyaku-test-stump-cache-forget-absent");

        let forgotten = forget(&work);

        crate::test_support::cleanup("taiyaku-test-stump-cache-forget-absent");
        assert!(forgotten.is_ok(), "{forgotten:?}");
    }

    /// A resolution survives a restart, including a recorded absence — which
    /// is the common case here and the one worth not re-asking about.
    #[test]
    fn resolutions_round_trip_beside_the_sidecar() {
        let work = work("taiyaku-test-stump-cache");
        let mut resolutions = Resolutions::new();
        resolutions.insert("hash-of-en-mono".into(), Some("a8ed51d8".into()));
        resolutions.insert("hash-of-raw-mono".into(), None);

        store(&work, "http://nas:10801", &resolutions).unwrap();
        let back = load(&work, "http://nas:10801");

        let path = path_for(&work);
        let existed = path.exists();
        crate::test_support::cleanup("taiyaku-test-stump-cache");

        assert!(existed, "must write beside the sidecar: {}", path.display());
        assert_eq!(
            back.get("hash-of-en-mono"),
            Some(&Some("a8ed51d8".to_string()))
        );
        assert_eq!(
            back.get("hash-of-raw-mono"),
            Some(&None),
            "an absence must be remembered, or every page turn asks again"
        );
    }

    /// A cache written against one instance says nothing about another. Book
    /// ids are that instance's, so believing them would serve pages from
    /// whatever happens to share an id — or 404 on everything.
    #[test]
    fn a_cache_from_a_different_instance_is_ignored() {
        let work = work("taiyaku-test-stump-cache-base");
        let mut resolutions = Resolutions::new();
        resolutions.insert("hash".into(), Some("id-on-the-old-server".into()));
        store(&work, "http://old:10801", &resolutions).unwrap();

        let back = load(&work, "http://new:10801");

        crate::test_support::cleanup("taiyaku-test-stump-cache-base");
        assert!(back.is_empty(), "a different base must not be trusted");
    }

    /// Nothing cached yet is not an error, and neither is a corrupt file: both
    /// mean "resolve again". A server that refused to start because a cache it
    /// can rebuild was truncated would be worse than one that rebuilds it.
    #[test]
    fn a_missing_or_unreadable_cache_is_empty_rather_than_fatal() {
        let work = work("taiyaku-test-stump-cache-bad");
        assert!(
            load(&work, "http://nas:10801").is_empty(),
            "nothing written yet"
        );

        std::fs::write(path_for(&work), b"{ truncated").unwrap();
        let back = load(&work, "http://nas:10801");

        crate::test_support::cleanup("taiyaku-test-stump-cache-bad");
        assert!(
            back.is_empty(),
            "a corrupt cache rebuilds rather than refusing to start"
        );
    }
}

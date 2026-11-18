//! Reading one file a page at a time for the fingerprint pass.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use sha2::{Digest, Sha256};
use taiyaku_library::fingerprints::{self, Outcome, Row, Unit};

use crate::fingerprint::{self, Border, PagePrint};

/// What reading one unit came to.
#[derive(Debug)]
pub(crate) enum Read {
    /// The row to store under the digest the read was given.
    Stored(Row),
    /// The unit changed or went while it was read, so what was read need not
    /// be its digest's content: nothing is stored, and it stays due.
    Gone,
    /// A folder whose pages do not hash to the digest the read was given:
    /// nothing is stored, and it stays due. With the error that ended the read
    /// before every page was hashed, where one did (O347); with none, its pages
    /// changed, as after a page is rewritten in place, which moves no stamp of
    /// the folder's, so no scan takes its digest again.
    NotItsDigest(Option<String>),
    /// `between` said to stop.
    Stopped,
}

/// Where a read is when it asks `between` whether to go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Before a page of the border sample is read.
    Sample,
    /// Before a page of the walk is read.
    Walk,
}

/// `unit`'s pages as the pass stores them under `digest`, stamped `build`:
/// the border its sampled pages agree on, as `common_border` samples them,
/// then every page in reading order, one in memory, each decoded through `large`.
///
/// `between` is asked before each page of the sample and of the walk, by its
/// index; `false` stops the read, and nothing is stored.
///
/// Keyed by digest, a row read while the unit changed could hold another
/// content's pages: so its stamp must be the scan's before and after, and a
/// folder's pages must hash to `digest`.
pub(crate) fn read_unit(
    unit: &Unit,
    digest: &str,
    build: Option<&str>,
    large: &LargePages,
    between: &mut dyn FnMut(Phase, usize) -> bool,
) -> Read {
    let path = unit.path.as_path();
    let stamp = Some((unit.size, unit.mtime));
    if fingerprints::stamp_of(path) != stamp {
        return Read::Gone;
    }
    // A folder has no one file whose stamp moves when a page is rewritten in
    // place, so the bytes its walk read are checked against the digest.
    let mut running = unit.folder.then(Sha256::new);
    // The error that ended the read before every page was hashed, if one did.
    let (prints, outcome, reason, failed) = match listing(path) {
        Err(e) => (Vec::new(), Outcome::Unreadable, Some(e.clone()), Some(e)),
        Ok((names, unread)) if names.is_empty() => (
            Vec::new(),
            Outcome::Unreadable,
            Some(if unread.is_empty() {
                "it holds no page".to_owned()
            } else {
                unread_reason(&unread)
            }),
            None,
        ),
        Ok((names, unread)) => {
            let Some(border) = sample(path, names.len(), large, between) else {
                return Read::Stopped;
            };
            let Some(walk) = walk(path, names.len(), border, large, between, running.as_mut())
            else {
                return Read::Stopped;
            };
            let outcome = outcome(&walk.prints, &unread);
            let reason = walk.reason(&unread);
            let failed = walk.failed.clone();
            (walk.prints, outcome, reason, failed)
        }
    };
    if fingerprints::stamp_of(path) != stamp {
        return Read::Gone;
    }
    if running.is_some_and(|hasher| hex(&hasher.finalize()) != digest) {
        return Read::NotItsDigest(failed);
    }
    Read::Stored(Row {
        sha256: digest.to_owned(),
        version: fingerprint::VERSION,
        outcome,
        reason,
        build: build.map(str::to_owned),
        pages: u32::try_from(prints.len())
            .expect("a member's page count fits a u32, as taiyaku_read counts pages"),
        prints: fingerprint::encode(&prints),
    })
}

/// `path`'s page names and the entries it holds that the reader does not read.
fn listing(path: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    Ok((
        taiyaku_read::member_page_names(path)?,
        taiyaku_read::member_unread_entries(path)?,
    ))
}

/// The border the sampled pages of `path`'s `count` agree on, each read and
/// decoded alone; `None` when `between` stops it. A sampled page that cannot
/// be read or decoded is left out, as `common_border` leaves one.
fn sample(
    path: &Path,
    count: usize,
    large: &LargePages,
    between: &mut dyn FnMut(Phase, usize) -> bool,
) -> Option<Border> {
    let mut seen = Vec::new();
    for i in fingerprint::border_sample(count) {
        if !between(Phase::Sample, i) {
            return None;
        }
        let Some(page) = u32::try_from(i)
            .ok()
            .and_then(|index| taiyaku_read::member_page(path, index).ok())
        else {
            continue;
        };
        let measured = large.run(page.bytes.len(), || {
            fingerprint::decode_luma(&page.bytes).map(|raw| fingerprint::flat_border(&raw))
        });
        if let Ok(border) = measured {
            seen.push(border);
        }
    }
    Some(fingerprint::median_border(&seen))
}

/// What a walk took from a unit's pages.
struct Walk {
    /// One per page in reading order, `None` for a page not hashed.
    prints: Vec<Option<PagePrint>>,
    /// Each page the decoder refused, by name, with its words.
    undecoded: Vec<(String, String)>,
    /// The read error that ended the walk early.
    failed: Option<String>,
}

impl Walk {
    /// Why the unit was not read whole, or `None` if it was.
    fn reason(&self, unread: &[String]) -> Option<String> {
        let mut why = Vec::new();
        match self.undecoded.as_slice() {
            [] => {}
            [(name, e)] => why.push(format!("{name} did not decode: {e}")),
            [(first, e), ..] => {
                let names: Vec<&str> = self.undecoded.iter().map(|(n, _)| n.as_str()).collect();
                why.push(format!("{} did not decode; {first}: {e}", names.join(", ")));
            }
        }
        why.extend(self.failed.clone());
        if !unread.is_empty() {
            why.push(unread_reason(unread));
        }
        (!why.is_empty()).then(|| why.join("; "))
    }
}

/// Every page of `path`'s `count` in reading order, less `border`, its bytes
/// fed to `running` when there is one; `None` when `between` stops it. A read
/// error ends the walk, and that page and every one after it are not hashed.
fn walk(
    path: &Path,
    count: usize,
    border: Border,
    large: &LargePages,
    between: &mut dyn FnMut(Phase, usize) -> bool,
    mut running: Option<&mut Sha256>,
) -> Option<Walk> {
    if !between(Phase::Walk, 0) {
        return None;
    }
    let mut prints = Vec::with_capacity(count);
    let mut undecoded = Vec::new();
    let mut stopped = false;
    let mut read_all = false;
    let walked = taiyaku_read::for_each_member_page(path, &mut |entry, bytes| {
        if let Some(hasher) = running.as_deref_mut() {
            hasher.update(bytes);
        }
        let described = large.run(bytes.len(), || {
            fingerprint::decode_luma(bytes).map(|raw| fingerprint::describe_page(&raw, border))
        });
        match described {
            Ok(print) => prints.push(Some(print)),
            Err(e) => {
                undecoded.push((entry.name.clone(), e));
                prints.push(None);
            }
        }
        // The walk lists the member again, so a page added since the read's
        // own listing would be read with no ask before it: the walk ends at
        // the count listed, and the stamp check after it finds the change.
        let next = prints.len();
        if next >= count {
            read_all = true;
            return Err(String::new());
        }
        // Asked here, before the walk reads the next page, so a stop costs
        // no page read past the one it follows.
        if !between(Phase::Walk, next) {
            stopped = true;
            return Err(String::new());
        }
        Ok(())
    });
    if stopped {
        return None;
    }
    let failed = walked.err().filter(|_| !read_all);
    if failed.is_some() && prints.len() < count {
        prints.resize(count, None);
    }
    Some(Walk {
        prints,
        undecoded,
        failed,
    })
}

/// Whole when every page is hashed and no entry is left unread, partial when
/// some page is hashed, unreadable when none is.
fn outcome(prints: &[Option<PagePrint>], unread: &[String]) -> Outcome {
    let hashed = prints.iter().flatten().count();
    if hashed == 0 {
        Outcome::Unreadable
    } else if hashed == prints.len() && unread.is_empty() {
        Outcome::Whole
    } else {
        Outcome::Partial
    }
}

fn unread_reason(unread: &[String]) -> String {
    format!("the reader does not read {}", unread.join(", "))
}

/// `bytes` in lower-case hex, as `taiyaku_read::member_digest` writes a digest.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// One decode at a time, across every worker sharing this, for a page at or
/// above the threshold; smaller pages decode side by side.
///
/// A page that large decodes to a buffer far past an ordinary page's, and
/// several at once could take memory that one at a time does not. The CLI's
/// `partition_by_size` keeps the same rule for its batches.
pub(crate) struct LargePages {
    threshold: usize,
    lock: Mutex<()>,
}

impl LargePages {
    pub(crate) fn new(threshold: usize) -> Self {
        Self {
            threshold,
            lock: Mutex::new(()),
        }
    }

    /// `f`, under the lock when `size` is at or above the threshold.
    ///
    /// A panic in `f` poisons the lock, which guards no data, so the next
    /// large page takes it all the same rather than every one after failing.
    pub(crate) fn run<T>(&self, size: usize, f: impl FnOnce() -> T) -> T {
        if size < self.threshold {
            return f();
        }
        let _alone = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        f()
    }
}

#[cfg(test)]
mod tests;

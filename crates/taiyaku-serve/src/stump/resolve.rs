//! Deciding which Stump book a sidecar member is, and proving it.
//!
//! Matching on name and size alone would be wrong on exactly the material this
//! corpus was built to hold — deliberate duplicate sets, and at least two files
//! that are not what their names claim. So a match is a candidate, and the
//! candidate is confirmed by comparing the SHA-256 of one page fetched through
//! each source.
//!
//! One page rather than the whole file: about 1 MB against 335 MB for this
//! volume, on a corpus with 430 MB pages — and it confirms the property that
//! actually matters, which is that page N agrees, rather than that two files
//! resemble each other.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::stump::cache::{self, Resolutions};
use crate::stump::{api, http::Transport};
use crate::works::Work;
use crate::{LocalLibrary, PageSource};

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    // Not `format!("{:x}", …)`: `digest` 0.11's `Output` no longer implements
    // `LowerHex`, matching the byte-by-byte formatting `taiyaku_read` already
    // uses for the same reason.
    hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// What resolving one work against Stump produced.
///
/// Split from a bare `Resolutions` so the caller in `serve()` can tell "every
/// member got an answer, absence included" apart from "a transport failure
/// cut this run short" — which is exactly what it needs to decide whether
/// trying the *next* work is worth another 30-second timeout, or whether the
/// instance is simply unreachable and every remaining work should be skipped
/// without asking at all.
#[derive(Debug)]
pub struct Resolved {
    pub resolutions: Resolutions,
    /// Set when a transport-level failure stopped this run before every
    /// member was asked about.
    pub aborted_by_transport: bool,
}

/// A `resolve_member` failure, distinguishing a transport-level outage — the
/// only thing allowed to abandon the rest of a run — from Stump (or the
/// local filesystem) answering with a refusal that must simply leave this
/// one member unresolved, to be retried next start.
struct ResolveError {
    message: String,
    transport: bool,
}

impl From<api::StumpError> for ResolveError {
    fn from(e: api::StumpError) -> Self {
        Self {
            message: e.message,
            transport: e.transport,
        }
    }
}

/// Resolve every member of a work, using and updating the cache beside it.
///
/// Members already answered for — including those answered "not in Stump" —
/// cost nothing. Only genuinely new members reach the network.
///
/// A member that cannot be resolved is left unanswered rather than failing the
/// load, and is asked about again on the next start: the composite falls
/// through to local disk in the meantime, which is where most of this library
/// lives anyway. A failure here would refuse to serve a work that reads
/// perfectly without Stump.
///
/// Unanswered is deliberately not the same as absent. "Stump has not got this"
/// is an answer and is cached; a transport failure, a GraphQL error or an
/// ambiguous candidate is the question going unasked, and caching that would
/// let one start against an unreachable instance disable Stump for this work
/// permanently and silently.
///
/// **A circuit breaker bounds the retrying, and only a transport failure may
/// trip it.** The distinction is the same one `composite.rs`'s request-path
/// cooldown already draws: an ambiguous candidate, an unconfirmed single
/// match, a GraphQL coercion error (a member above ~2.1 GB, say), a locally
/// missing member file or a corrupt local archive are all Stump — or the
/// filesystem — *answering*, and must simply leave that one member
/// unresolved while the rest of the run continues. Only `Transport::post_json`
/// or `Transport::get_bytes` itself failing means the instance is
/// unreachable, and that is what abandons the rest of this run's unresolved
/// members — against an unreachable Stump that is one failed lookup at
/// startup instead of one per member, which on a large library is minutes of
/// `ureq` timeouts before the listener binds. Members never attempted stay
/// absent from the map exactly like a member that was attempted and refused,
/// so the next start retries all of them; nothing here changes what gets
/// retried, only how much of one run is spent finding out — and, before this
/// fix, a single semantic refusal anywhere in a work silently degraded that
/// entire work to local-only, forever, on every start.
///
/// `report` is called once per member that actually has to be asked about,
/// with its path, its position among those members and how many there are —
/// O8. Members answered from the cache are not reported, because a run that
/// asks nothing should say nothing: a cached start is the common case and a
/// line per member on every one of them is noise that trains a reader to
/// ignore the lines that matter.
pub fn resolve_work(
    transport: &dyn Transport,
    work: &Work,
    base: &str,
    report: &mut dyn FnMut(&str, usize, usize),
) -> Resolved {
    let mut resolutions = cache::load(work, base);
    let root = work
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut learned = false;
    let mut aborted_by_transport = false;
    let sidecar = work.sidecar();

    // Counted before the walk, so the first line can say how many there are
    // rather than only how many have gone by. Deduplicated on `sha256` for
    // the same reason the cache is keyed on it: two members with the same
    // hash are one file and are asked about once, so counting them twice
    // would promise a total the walk never reaches.
    let outstanding: std::collections::HashSet<&str> = sidecar
        .editions
        .values()
        .flat_map(|edition| &edition.members)
        .filter(|member| !resolutions.contains_key(&member.sha256))
        .map(|member| member.sha256.as_str())
        .collect();
    let outstanding = outstanding.len();
    let mut asked = 0;

    'lookups: for edition in sidecar.editions.values() {
        for member in &edition.members {
            if resolutions.contains_key(&member.sha256) {
                continue;
            }
            asked += 1;
            report(&member.path, asked, outstanding);
            match resolve_member(transport, root, member) {
                // An answer, cached: either Stump has this member or it
                // genuinely does not, and neither changes on the next run.
                Ok(answer) => {
                    resolutions.insert(member.sha256.clone(), answer);
                    learned = true;
                }
                // A transport-level failure: the instance itself could not be
                // reached, so trying every remaining member would not learn
                // anything new, only spend the same timeout again for each of
                // them. This is the only case that trips the breaker.
                Err(ResolveError {
                    message,
                    transport: true,
                }) => {
                    eprintln!(
                        "taiyaku: {} not resolved against stump: {message} — stump appears \
                         unreachable; abandoning the rest of this run's unresolved members; \
                         they will be retried next start",
                        member.path
                    );
                    aborted_by_transport = true;
                    break 'lookups;
                }
                // NOT an answer, and so NOT cached, but also NOT an outage:
                // Stump (or the local filesystem) answered — an ambiguous
                // candidate, an unconfirmed match, a GraphQL error, a missing
                // or corrupt local file — which must leave only *this*
                // member unresolved rather than abandoning every member
                // after it.
                Err(ResolveError {
                    message,
                    transport: false,
                }) => {
                    eprintln!(
                        "taiyaku: {} not resolved against stump: {message} — stump answered, \
                         which is not an outage; left unresolved and will be retried next start",
                        member.path
                    );
                }
            }
        }
    }

    if learned {
        // Not `?`. The cache is derived data — `load` already treats every
        // read failure as "resolve again" — so a full disk or a read-only
        // library directory must not refuse to serve a work that reads
        // perfectly from disk without Stump at all. This function's caller
        // runs before the listener binds, so propagating here would abort
        // startup for every work rather than slow one of them down.
        if let Err(why) = cache::store(work, base, &resolutions) {
            eprintln!(
                "taiyaku: could not remember stump resolutions for {}: {why}",
                work.path.display()
            );
        }
    }
    Resolved {
        resolutions,
        aborted_by_transport,
    }
}

/// One member: find a candidate, then make it prove itself.
fn resolve_member(
    transport: &dyn Transport,
    root: &std::path::Path,
    member: &taiyaku_core::Member,
) -> Result<Option<String>, ResolveError> {
    let path = root.join(&member.path);
    // A locally missing member, or one whose name has no stem, is Stump (or
    // rather, the filesystem) answering with a refusal — not evidence the
    // *instance* is unreachable — so this must not trip the circuit breaker.
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| ResolveError {
            message: format!("{}: no file name", path.display()),
            transport: false,
        })?;
    let size = std::fs::metadata(&path)
        .map_err(|e| ResolveError {
            message: format!("{}: {e}", path.display()),
            transport: false,
        })?
        .len();

    let Some(book) = api::find_book(transport, &stem, size, member.pages)? else {
        return Ok(None);
    };

    // The confirmation: the same page through each source. Both calls take
    // taiyaku's 0-based index; `page_bytes` speaks Stump's 1-based numbering
    // internally so no caller has to remember to.
    //
    // A corrupt local archive is a refusal too — the transport was never
    // touched — so it is wrapped the same way as the filesystem errors
    // above rather than being allowed to abandon the run.
    let ours = LocalLibrary
        .page(root, member, 0)
        .map_err(|e| ResolveError {
            message: e,
            transport: false,
        })?;
    let theirs = api::page_bytes(transport, &book.id, 0)?;
    if digest(&ours.bytes) != digest(&theirs.bytes) {
        eprintln!(
            "taiyaku: {} matches stump book {} by name and size, but their first pages differ — treating it as absent",
            member.path, book.path
        );
        return Ok(None);
    }
    Ok(Some(book.id))
}

#[cfg(test)]
mod tests;

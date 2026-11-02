//! One work, two sources, chosen per member.
//!
//! Stump indexes the English releases; the raws and fan translations taiyaku
//! aligns them *against* are not in it, and a library of English releases is
//! what a Stump instance normally is.
//!
//! So a source that answered for a whole work would answer for no work here.
//! Routing per member is what makes taiyaku run beside Stump rather than
//! requiring it.

use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use taiyaku_core::Member;

use crate::stump::cache::Resolutions;
use crate::stump::{api, http::Transport};
use crate::{LocalLibrary, PageSource, ServedPage};

/// A configured Stump instance and what has been resolved against it.
///
/// `down_until` is the request path's circuit breaker, and it exists for the
/// same reason as the one in `resolve_work`. Falling back to disk is only
/// useful if the fallback is *quick*: with a resolution cache already on disk
/// and the instance unreachable, every page would otherwise pay the full GET
/// timeout before disk answered, on every request, forever — and a reader
/// waiting that long for each page cannot tell that from broken.
///
/// **Only a transport-level failure latches.** Stump answering 404 for a page
/// a book does not have is an answer, not an outage, and must not demote the
/// whole instance.
#[derive(Debug, Clone)]
pub struct StumpSide {
    pub transport: Arc<dyn Transport>,
    /// What has been resolved so far.
    ///
    /// **Shared and mutable because of O8.** Resolution runs behind the bound
    /// listener: the map starts empty, the server answers from disk
    /// immediately, and each work's answers arrive here as that work finishes
    /// being asked about. A member not yet resolved is indistinguishable from
    /// one Stump does not have — both are `None` from `book_for`, and both
    /// mean "read it off disk" — which is why the window is safe rather than
    /// merely short.
    resolutions: Arc<RwLock<Resolutions>>,
    /// How long a transport failure suppresses further attempts.
    pub cooldown: Duration,
    /// When the suppression lifts, if it is in force.
    pub down_until: Arc<Mutex<Option<Instant>>>,
}

/// How long a transport failure suppresses further attempts.
///
/// **Longer than `stump::http::GET_TIMEOUT_SECONDS`, deliberately.** The latch
/// only suppresses once a failure has returned, so a reader meets one full
/// timeout per cooldown while Stump is down. A cooldown shorter than the
/// timeout means the stalls dominate and the latch buys almost nothing.
pub const DOWN_COOLDOWN: Duration =
    Duration::from_secs(crate::stump::http::GET_TIMEOUT_SECONDS * 3);

impl StumpSide {
    /// A side with the cooldown latch clear, ready for its first request.
    pub fn new(
        transport: Arc<dyn Transport>,
        resolutions: Resolutions,
        cooldown: Duration,
    ) -> Self {
        Self {
            transport,
            resolutions: Arc::new(RwLock::new(resolutions)),
            cooldown,
            down_until: Arc::new(Mutex::new(None)),
        }
    }

    /// Take on what a work has just resolved to.
    ///
    /// Extends rather than replaces, because resolution reports per work and a
    /// library is several: replacing would make each work's answers discard
    /// the one before it. Called from the background task `serve` spawns; a
    /// request arriving mid-update either sees a member or does not, and
    /// "does not" is already the disk path.
    pub fn learn(&self, resolutions: Resolutions) {
        self.resolutions
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(resolutions);
    }

    /// Whether a prior transport failure is still suppressing attempts.
    fn is_down(&self) -> bool {
        matches!(*self.down_until.lock().unwrap(), Some(until) if Instant::now() < until)
    }

    /// A transport-level failure just happened against `id`: latch the
    /// cooldown so the next requests go straight to disk.
    ///
    /// This is only ever called after an attempt that was actually made, and
    /// an attempt is only made when `is_down` was false — so a *suppressed*
    /// request never logs, which is the noise this exists to avoid.
    ///
    /// It is not, however, one log line per outage. `is_down` and this call
    /// are a non-atomic read-modify-write, so every request already in flight
    /// when the first failure returns also attempts and also logs. Bounded by
    /// how many requests fit inside one timeout, self-healing, and not worth
    /// a lock held across the network to prevent — but the guarantee is
    /// "never on the suppressed path", not "exactly once".
    fn note_failure(&self, id: &str, why: &str) {
        eprintln!(
            "taiyaku: stump unreachable (book {id}: {why}) — suppressing further attempts for \
             {:?}",
            self.cooldown
        );
        *self.down_until.lock().unwrap() = Some(Instant::now() + self.cooldown);
    }

    /// A request just succeeded: clear the latch, and say so if it was set.
    /// A NAS reboot should be visible in both directions.
    fn note_success(&self) {
        if self.down_until.lock().unwrap().take().is_some() {
            eprintln!("taiyaku: stump reachable again");
        }
    }
}

/// Pages from Stump where it has them, from disk where it does not.
#[derive(Debug, Clone)]
pub struct CompositeSource {
    local: LocalLibrary,
    stump: Option<StumpSide>,
}

impl CompositeSource {
    #[must_use]
    pub fn new(local: LocalLibrary, stump: Option<StumpSide>) -> Self {
        Self { local, stump }
    }

    /// The Stump book this member is, if it is one *and* it has been resolved
    /// yet.
    ///
    /// Returns an owned id rather than a borrow: the map sits behind a lock
    /// (see `StumpSide::resolutions`), and holding a read guard across the
    /// network fetch below would block every resolution the background task
    /// is trying to record. One `String` per page request is not a cost worth
    /// a borrow.
    fn book_for(&self, member: &Member) -> Option<String> {
        self.stump
            .as_ref()?
            .resolutions
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&member.sha256)?
            .clone()
    }
}

impl PageSource for CompositeSource {
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String> {
        if let (Some(side), Some(id)) = (self.stump.as_ref(), self.book_for(member)) {
            if side.is_down() {
                // The latch is in force: go straight to disk without
                // touching the network. Not logging here is deliberate — one
                // log per suppressed request is exactly the noise the latch
                // exists to trade for a slow request instead.
                return self.local.page(root, member, within);
            }
            // `within` stays 0-based here. `page_bytes` owns the conversion to
            // Stump's 1-based numbering, so this call site cannot get it wrong.
            match api::page_bytes(side.transport.as_ref(), &id, within) {
                // **The declared `Content-Type` is not trusted here.**
                // `source.rs` sniffs local page bytes because a declared type
                // is frequently wrong (some page entries are PNGs named
                // `.jpg`), and the design doc records that a stale path on the
                // real instance falls through to Stump's SPA and returns HTML
                // with a 200. Believing the header there would serve a reader
                // that HTML as every page, and with a populated
                // `taiyaku.stump.json` the resolve-time page-hash confirmation
                // never runs again to catch it. Sniffing is applied only here,
                // at the live serving path — not inside `page_bytes` itself,
                // which `resolve_member` also calls for its own byte-for-byte
                // confirmation, a strictly stronger check that must not be
                // weakened into an unsniffable-content refusal.
                Ok(page) => match crate::source::sniffed_type(&page.bytes) {
                    Some(sniffed) => {
                        side.note_success();
                        return Ok(ServedPage {
                            content_type: sniffed.to_string(),
                            bytes: page.bytes,
                        });
                    }
                    None => {
                        eprintln!(
                            "taiyaku: stump book {id} page {within}: response declared \
                             `{}` but did not sniff as an image — falling back to disk",
                            page.content_type
                        );
                        // Stump answered, just not with a page — most likely
                        // its SPA's HTML after a moved API path. Not a
                        // transport failure, so this must not latch the
                        // cooldown; falling through to disk below.
                    }
                },
                // Disk is the fallback, not the failure. A member that is also
                // on disk must not become unreadable because the NAS rebooted,
                // and this library keeps most of its members there anyway.
                Err(err) => {
                    eprintln!(
                        "taiyaku: stump book {id} page {within}: {} — falling back to disk",
                        err.message
                    );
                    // A 404 is Stump answering "no", not Stump being down —
                    // latching on it would demote the whole instance for one
                    // member the library genuinely does not have there.
                    if err.transport {
                        side.note_failure(&id, &err.message);
                    }
                }
            }
        }
        self.local.page(root, member, within)
    }
}

#[cfg(test)]
mod tests;

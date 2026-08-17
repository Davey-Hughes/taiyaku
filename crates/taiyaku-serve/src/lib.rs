//! `taiyaku serve` — the sidecar, its pages, and where you were, over HTTP.
//!
//! Everything above `PageSource` is indifferent to where pages come from, which
//! is what lets taiyaku run beside Stump now and replace it later without a
//! redesign. See `docs/superpowers/specs/2026-07-30-reader-design.md`.

mod alignments;
mod cache_root;
mod composite;
#[cfg(test)]
mod corpus;
mod cover;
/// A work's notes, beside its sidecar.
///
/// Public where `progress` is private, and deliberately: `progress::load` and
/// `progress::store` are reachable from the crate root as `Progress` plus two
/// routes, but `load`/`store`/`to_json` re-exported there would be three names
/// too vague to sit beside `load_works`. The module path says what they load.
pub mod notes;
mod opds;
mod progress;
mod reqlog;
mod routes;
mod source;
mod stump;
mod truth;
mod works;

use std::sync::Arc;

pub use composite::{CompositeSource, StumpSide};
pub use reqlog::{Entry, Log};
pub use routes::build_router;
pub use source::{LocalLibrary, PageSource, ServedPage};
// `Progress` moved to `taiyaku-library` (Task 9's `state::progress` needs it
// there); re-exported from its new home so `crate::Progress` still resolves.
pub use taiyaku_library::Progress;
// No `from_library` here. It existed as the named entry point for "a
// `ServeState` off the database" and never acquired a production caller:
// `taiyaku serve <sidecar>...` builds its `Work`s one path at a time in
// `taiyaku-cli`'s `serve_work`, because it serves the works it was *handed*
// rather than every work the library holds, and it already has each work's
// path from the command line. Two projections of one thing is how they come
// to disagree, and this was the one nothing ran (whole-branch review 2,
// Minor 4). A `serve --db` with no paths would want it back; it is six lines
// of `SELECT id, slug, sidecar_path` and `project::sidecar`, and it should be
// written then, against whatever that command decides about a work whose
// `sidecar_path` is NULL.
pub use works::{Work, load_works, work_id};

/// Everything a request needs: which works exist, and where pages come from.
#[derive(Debug)]
pub struct ServeState {
    /// Where the progress, notes and truth routes (Task 13) read and write
    /// through — and, since Task 15, the same library `works` below was
    /// imported into (`taiyaku-cli`'s `serve` opens it, imports every
    /// sidecar path it was handed through `library::import_canonical`, and
    /// passes the open library in here), rather than a fresh in-memory one
    /// this function used to open for itself. That is what makes a bridge
    /// row `works::row_id_for_write` might still mint (a write reaching a
    /// work through some path that bypassed import — `test_support::state`'s
    /// fixtures do this on purpose, to test the bridge in isolation) land on
    /// the *same* database a later `taiyaku library import` reconciles
    /// against, rather than one that vanishes with the process.
    pub library: Arc<taiyaku_library::Library>,
    pub works: Vec<Work>,
    pub source: Arc<dyn PageSource>,
    /// Where generated covers are kept between runs.
    pub cache_dir: std::path::PathBuf,
    /// Set when `--log-requests` was given; otherwise nothing is recorded.
    pub log: Option<Arc<crate::reqlog::Log>>,
}

impl ServeState {
    #[must_use]
    pub fn work(&self, id: &str) -> Option<&Work> {
        self.works.iter().find(|w| w.id == id)
    }

    /// Where generated filmstrip tiles are kept between runs.
    ///
    /// A sibling of `cache_dir` rather than the same directory, so that the
    /// two can be thrown away separately — because only one of them is cheap
    /// to rebuild. The covers are one per edition and 165 KB for the whole
    /// library; the tiles and reading-size pages beside them are 1.12 MiB per
    /// 195-page edition and cost 126 ms a page to regenerate against 10 ms
    /// warm, measured over 235 real generations. One shared directory would
    /// make clearing the cheap half impossible without taking the expensive
    /// half with it, which is exactly the bluntness this arrangement avoids.
    ///
    /// O11 made this argument about `.taiyaku-cache`, where the server's
    /// covers sat beside the CLI's fingerprint cache. They no longer share a
    /// tree — `cache_root` moved the server's under the platform cache root —
    /// and the argument outlived the move because it was never about which
    /// tree the two were in.
    #[must_use]
    pub fn thumb_cache_dir(&self) -> std::path::PathBuf {
        self.cache_dir.with_file_name("thumbs")
    }

    /// The work a route names, or the message to send back when it does not
    /// exist.
    ///
    /// Four call sites — the `/api/…` page route, the OPDS work feed, its
    /// page stream and its cover — each typed their own copy of `` no work
    /// `{id}` `` before this existed, which is exactly the drift the doc
    /// comment on `routes.rs`'s own `work()` helper already warned about:
    /// one place decides what a request for a missing work gets back, so
    /// four routes cannot drift into four different answers to the same
    /// question.
    ///
    /// # Errors
    ///
    /// Returns an error, worded `` no work `{id}` ``, if `id` names no work
    /// this server was started with.
    pub fn work_or_missing(&self, id: &str) -> Result<&Work, String> {
        self.work(id).ok_or_else(|| format!("no work `{id}`"))
    }

    /// One page of an edition, by its index in that edition's flat sequence.
    ///
    /// This is where a flat index becomes a member and an offset, once, for
    /// every source and every route — the API, the OPDS stream and the cover
    /// all arrive here. The failure messages name what was missing, because a
    /// reader shown a blank page with no explanation is the failure this
    /// project exists to avoid.
    ///
    /// # Errors
    ///
    /// Returns an error if `edition` names no edition in `work`'s sidecar, if
    /// `index` is at or past that edition's total page count (the message
    /// then states how many pages it has), or if the underlying source
    /// (`PageSource::page`) fails to produce the page it located — a local
    /// read failure, or, over Stump, a failed fetch. Every case appends
    /// `work.path` to the source's own message.
    ///
    /// **Both callers in `routes.rs` — `get_page` and `get_thumb` — map every
    /// `Err` from this function to `404 not_found`, never `500`.** A local
    /// read failure that has nothing to do with what the client asked for —
    /// a permission refusal, a corrupted archive — is therefore reported to
    /// a client identically to a page that genuinely does not exist.
    pub fn page(&self, work: &Work, edition: &str, index: u32) -> Result<ServedPage, String> {
        let sidecar = work.sidecar();
        let found = sidecar
            .editions
            .get(edition)
            .ok_or_else(|| format!("no edition `{edition}` in {}", work.path.display()))?;
        let (position, within) = found.locate(index).ok_or_else(|| {
            format!(
                "`{edition}` has {} pages; there is no page {index}",
                found.total_pages()
            )
        })?;
        let root = work.path.parent().unwrap_or(std::path::Path::new("."));
        self.source
            .page(root, &found.members[position], within)
            .map_err(|e| format!("{e} ({})", work.path.display()))
    }
}

/// How to reach a Stump instance, if one is configured.
#[derive(Debug, Clone)]
pub struct StumpConfig {
    pub base: String,
    pub key: String,
}

impl StumpConfig {
    /// `base` is normalised once, here — a trailing slash trimmed — rather
    /// than leaving each consumer to do it separately.
    ///
    /// Before this existed, `UreqTransport::new` trimmed its own copy of
    /// `base` for the transport (`stump/http.rs`), while `resolve_work`'s
    /// cache key (`lib.rs`'s call into `resolve::resolve_work`) used whatever
    /// was typed. `https://nas:10801/` and `https://nas:10801` were
    /// therefore one instance to every request and two to the resolution
    /// cache — so typing the slash once silently discarded every prior
    /// resolution, forever, with nothing to say so.
    pub fn new(base: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            key: key.into(),
        }
    }
}

/// Bind and serve until interrupted.
///
/// Separate from `build_router` so the router's behaviour is driven directly,
/// without a socket, by every test that is about a route. **One test is about
/// the bind itself** — `serve_binds_the_address_and_answers_on_it` — because
/// with none, this whole function could be replaced by `Ok(())`, starting no
/// server at all, and nothing else in the crate would notice.
///
/// `library` is the caller's already-open library — `taiyaku-cli`'s `serve`
/// opens it at the resolved `--db` path and imports every `works` entry into
/// it before calling this, which is Task 15's "`taiyaku serve <sidecar>...`
/// keeps working: it imports those paths and serves them." This function no
/// longer opens one of its own; the tests in this crate that build a
/// `ServeState` without going through an import (`test_support::state`, most
/// of this file's own fixtures) still work unchanged, by passing an empty
/// `Library::open_in_memory()` — this parameter only says *which* library to
/// serve out of, not that one must hold every work already.
///
/// # Errors
///
/// Returns an error if `log_requests` is `Some` and the file it names cannot
/// be opened or created (see `reqlog::Log::new`), if binding `addr` fails —
/// the port already in use, the address not this host's, or a privileged
/// port without the permission it needs — or if the server's accept loop,
/// run by `axum::serve`, ends in an I/O error. The bind failure and the
/// accept-loop failure are both worded `{addr}: {e}`, since a process that
/// may serve several addresses over its life prints nothing else to say
/// which one failed.
pub async fn serve(
    library: Arc<taiyaku_library::Library>,
    works: Vec<Work>,
    addr: &str,
    stump: Option<StumpConfig>,
    log_requests: Option<std::path::PathBuf>,
    refresh_stump: bool,
    cache: Option<std::path::PathBuf>,
) -> Result<(), String> {
    // Opened before anything else so a bad `--log-requests` path is a
    // startup failure that can be acted on, rather than an hour of reading
    // on an iPad producing a silently empty file.
    let log = match log_requests {
        Some(path) => Some(Arc::new(reqlog::Log::new(&path)?)),
        None => None,
    };

    // What the background resolution will need, kept aside so it can be
    // started *after* the bind below rather than before it.
    let mut pending: Option<(Arc<dyn stump::http::Transport>, StumpSide, String)> = None;

    let source: Arc<dyn PageSource> = match stump {
        None => {
            // Not silent. A flag that quietly does nothing is worse than one
            // that refuses: the person who typed it goes on believing the
            // stale answers were thrown away.
            if refresh_stump {
                eprintln!(
                    "taiyaku: --refresh-stump has no effect without --stump-url; nothing was \
                     forgotten"
                );
            }
            Arc::new(CompositeSource::new(LocalLibrary, None))
        }
        Some(config) => {
            // O11: before resolution, not after, so the run that follows is
            // the one that re-asks.
            if refresh_stump {
                forget_all_works(&works);
            }
            let transport: Arc<dyn stump::http::Transport> = Arc::new(
                stump::http::UreqTransport::new(config.base.clone(), config.key),
            );
            // O8: constructed with **nothing resolved**. Resolution used to
            // run here, before the bind, so a cold start against a reachable
            // instance cost one GraphQL round trip per member — measured at
            // 3.15 s for one nine-member work — and a start against an
            // unreachable one cost a full 30 s POST timeout before the
            // listener existed, on every start, to reach pages that were
            // sitting on local disk the whole time.
            let side = StumpSide::new(
                transport.clone(),
                stump::cache::Resolutions::new(),
                composite::DOWN_COOLDOWN,
            );
            pending = Some((transport, side.clone(), config.base.clone()));
            Arc::new(CompositeSource::new(LocalLibrary, Some(side)))
        }
    };

    // Resolved before the bind, so a person who is about to be surprised by
    // where their pages went reads it above `listening` rather than below it.
    let xdg = std::env::var_os("XDG_CACHE_HOME");
    let home = std::env::var_os("HOME");
    let resolved = cache_root::cache_root(cache.as_deref(), xdg.as_deref(), home.as_deref());
    // The rule two hundred lines up, applied to a default rather than to a
    // flag: a thing that quietly does something surprising is worse than one
    // that says so. The fallback branch is the only one that still follows the
    // working directory — a relative variable is ignored rather than joined,
    // and a relative `--cache` was typed on purpose — which is the defect that
    // module was written to remove, so it is the one branch that must not be
    // silent. Which branch that is lives in `cache_root::notice`, where a test
    // can assert it; deciding it here would put it back inside an `eprintln!`.
    if let Some(notice) = cache_root::notice(&resolved) {
        eprintln!("{notice}");
    }
    // Created here rather than left to the first request that wants it.
    // `cover.rs` treats a cache directory it cannot make as non-fatal and
    // silent, which is right per request and hides the startup case entirely:
    // `--cache` naming a file, or a directory under a read-only mount, would
    // otherwise serve perfectly and regenerate every page forever. Reported
    // and survived rather than refused, for the same reason the fallback is a
    // fallback — a cache is derived data.
    if let Some(complaint) = cache_root::create(&resolved) {
        eprintln!("{complaint}");
    }

    let state = Arc::new(ServeState {
        library,
        works,
        source,
        cache_dir: resolved.covers(),
        log,
    });
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("{addr}: {e}"))?;
    eprintln!("taiyaku serve listening on http://{addr}");

    // O8: after the bind, deliberately — including the order of these two
    // lines on stderr, since "listening" arriving before the resolution
    // chatter is how a person watching a cold start can see that the wait is
    // no longer theirs. `spawn_blocking` because resolution is a network
    // round trip *and* a local page hash per member, neither of which belongs
    // on an async worker; `get_page` and the OPDS handlers already keep to
    // that boundary for the same reason (O12).
    if let Some((transport, side, base)) = pending {
        let resolving = Arc::clone(&state);
        tokio::task::spawn_blocking(move || {
            resolve_all_works(
                transport.as_ref(),
                &resolving.works,
                &base,
                &mut |learned| {
                    side.learn(learned);
                },
            );
        });
    }
    // Named the same way the bind failure is. Both are reported by a process
    // that may be serving several addresses over its life and prints nothing
    // else about them; "Address already in use" without one is a message that
    // has to be guessed at.
    axum::serve(listener, build_router(state))
        .await
        .map_err(|e| format!("{addr}: {e}"))
}

/// Throw away every work's remembered Stump resolutions — `--refresh-stump`.
///
/// **A work whose file cannot be removed is reported and does not stop the
/// others.** The same reasoning `resolve_work` applies to a cache it cannot
/// write: these are derived answers, and refusing to serve a whole library
/// because one directory is read-only would trade a stale answer for no
/// answer at all. A work with nothing recorded is not a failure at all — see
/// `cache::forget`.
///
/// Separate from `serve` for the reason `resolve_all_works` is: `serve` binds
/// a socket and so is not something a test drives, and the loop is where the
/// behaviour worth pinning lives.
fn forget_all_works<'a>(works: impl IntoIterator<Item = &'a Work>) {
    for work in works {
        if let Err(why) = stump::cache::forget(work) {
            eprintln!(
                "taiyaku: could not forget stump resolutions for {}: {why}",
                work.id
            );
        }
    }
}

/// Resolve every work against Stump, in order, stopping as soon as one work
/// reports a transport-level failure.
///
/// Important 3 (whole-branch review): **the breaker spans works, not just
/// one work's members.** `resolve_work` already stops asking after the first
/// transport-level failure within a work; without hoisting it here too, an
/// unreachable instance would still pay one 30-second POST timeout *per
/// work* before the listener binds — twenty works is ten minutes, with
/// nothing printed until each one finishes. Once any work reports
/// `aborted_by_transport`, every later work is skipped without being asked
/// at all, and that is said once rather than once per skipped work.
///
/// Separate from `serve` so this loop can be driven by a `FakeTransport`
/// without a socket — `serve`'s only remaining job around this call is
/// turning the aggregated `Resolutions` into a `CompositeSource`.
fn resolve_all_works(
    transport: &dyn stump::http::Transport,
    works: &[Work],
    base: &str,
    learned: &mut dyn FnMut(stump::cache::Resolutions),
) {
    for (index, work) in works.iter().enumerate() {
        // O8: this runs before the listener binds, so it is time the server
        // spends unable to answer anything. It used to print one line per
        // work, *after* that work finished — so a work with many unresolved
        // members was silent for its whole duration and a slow resolution
        // was indistinguishable from a hung one. Members answered from the
        // cache print nothing, so a warm start is as quiet as it was.
        let mut report = |member: &str, nth: usize, total: usize| {
            eprintln!(
                "{}: resolving {member} against stump ({nth} of {total})",
                work.id
            );
        };
        let resolved = stump::resolve::resolve_work(transport, work, base, &mut report);
        let known = resolved
            .resolutions
            .values()
            .filter(|v| v.is_some())
            .count();
        // The true member count, not `resolved.resolutions.len()`: the
        // circuit breaker in `resolve_work` can abandon a run partway
        // through, leaving members entirely absent from the map. A wholly
        // unreachable Stump would otherwise print "0 of 0 members from
        // stump", which reads as a work with no members rather than a run
        // that gave up.
        let total: usize = work
            .sidecar()
            .editions
            .values()
            .map(|e| e.members.len())
            .sum();
        eprintln!("{}: {known} of {total} members from stump", work.id);
        let aborted = resolved.aborted_by_transport;
        // O8: handed over as this work finishes rather than accumulated and
        // returned at the end, so the works already resolved are usable while
        // the rest are still being asked about. Reported even on an abort,
        // since a run that gave up partway still learned whatever it got to.
        learned(resolved.resolutions);
        if aborted {
            if let Some(notice) = skipping_notice(works.len(), index) {
                eprintln!("{notice}");
            }
            break;
        }
    }
}

/// What to say when a run gives up partway, or `None` when it gave up on the
/// last work and there is nothing left to skip.
///
/// `works.len() - index - 1` and the `> 0` test that guards it were both
/// invisible to the suite: they decide only the wording of a line on stderr,
/// and the abort test asserts which requests were made rather than what was
/// printed. Extracted so the arithmetic is a value something can check —
/// including the case that matters most, aborting on the final work, where
/// every mutation of the subtraction produces a sentence promising to skip
/// works that do not exist.
fn skipping_notice(total: usize, index: usize) -> Option<String> {
    let remaining = total.saturating_sub(index + 1);
    (remaining > 0).then(|| {
        format!(
            "taiyaku: stump appears unreachable — skipping stump resolution for the \
             remaining {remaining} work(s); they will read from disk until the next start"
        )
    })
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;

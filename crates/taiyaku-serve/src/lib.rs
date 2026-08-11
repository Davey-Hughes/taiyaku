//! `taiyaku serve` — the sidecar, its pages, and where you were, over HTTP.
//!
//! Everything above `PageSource` is indifferent to where pages come from, which
//! is what lets taiyaku run beside Stump now and replace it later without a
//! redesign. See `docs/superpowers/specs/2026-07-30-reader-design.md`.

/// Write a file so a crash cannot publish half of one.
///
/// Public where most of this crate's modules are private, and unlike `notes`,
/// which is public to keep three vague names off the crate root rather than
/// for any caller outside this crate. This one is genuinely cross-crate:
/// `taiyaku-cli`'s `credits --write` rewrites a whole sidecar in place, the
/// exact case this module's own doc names as the worst one, and the
/// alternative to exporting it is a fifth copy of write-and-rename in another
/// crate. The module path says what the function guarantees, which a re-export
/// at the crate root as a bare `write` would not.
pub mod atomic;
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
mod works;

use std::sync::Arc;

pub use composite::{CompositeSource, StumpSide};
pub use progress::Progress;
pub use reqlog::{Entry, Log};
pub use routes::build_router;
pub use source::{LocalLibrary, PageSource, ServedPage};
pub use works::{Work, load_works, work_id};

/// Everything a request needs: which works exist, and where pages come from.
#[derive(Debug)]
pub struct ServeState {
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
pub(crate) mod test_support {
    use std::io::Write;

    use crate::{ServeState, Work};

    /// A work of two cbz members, four pages, on disk under a named temp dir.
    ///
    /// Two members deliberately: the offset arithmetic that picks which member
    /// holds page N is what most of these tests are about, and one member
    /// cannot exercise it. Each entry's bytes are its own name, so a test can
    /// tell which page it got.
    ///
    /// Shared rather than local to one test module because Task 5's route
    /// tests need the same work.
    pub(crate) fn work(dir: &str) -> Work {
        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        for (name, entries) in [
            ("c1.cbz", ["a1.jpg", "a2.jpg"]),
            ("c2.cbz", ["b1.jpg", "b2.jpg"]),
        ] {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(name)).unwrap());
            for entry in entries {
                zip.start_file(entry, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(entry.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }

        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
        let path = root.join("taiyaku.json");
        std::fs::write(&path, json).unwrap();

        Work::new(
            "t".into(),
            path,
            taiyaku_core::Sidecar::from_json(json).unwrap(),
        )
    }

    /// The fixture work, wired to a `LocalLibrary`, ready for `build_router`.
    pub(crate) fn state(dir: &str) -> ServeState {
        ServeState {
            works: vec![work(dir)],
            source: std::sync::Arc::new(crate::LocalLibrary),
            cache_dir: std::env::temp_dir().join(dir).join("covers"),
            log: None,
        }
    }

    /// The shared fixture plus a second edition and a solved edge between
    /// them, which is the minimum a correction needs to address.
    ///
    /// `raw` is four pages across two members, as `work` builds it; `en` is
    /// the same two members under a second key, so no new archives are needed
    /// and the pages a correction addresses really exist.
    pub(crate) fn state_with_two_editions(dir: &str) -> ServeState {
        let mut state = state(dir);
        let path = state.works[0].path.clone();
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] },
            "en": { "label": "En", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] }
          ]
        }"#;
        std::fs::write(&path, json).unwrap();
        state.works = vec![crate::Work::new(
            "t".into(),
            path,
            taiyaku_core::Sidecar::from_json(json).unwrap(),
        )];
        state
    }

    /// A pixel channel value synthesized for `real_page_bytes` below.
    ///
    /// # Panics
    ///
    /// Panics if `v` exceeds `u8::MAX`. It cannot: `real_page_bytes` multiplies
    /// a coordinate `from_fn` bounds to `0..16` by 16, topping out at `15 * 16
    /// == 240`.
    fn channel(v: u32) -> u8 {
        u8::try_from(v).expect("channel value exceeds u8::MAX")
    }

    /// A small real, decodable page image — same recipe as
    /// `cover::tests::real_page_bytes`, duplicated here rather than exposed
    /// across the module boundary that keeps `cover`'s test helpers private
    /// to `cover`.
    fn real_page_bytes() -> Vec<u8> {
        let page = image::RgbImage::from_fn(16, 16, |x, y| {
            let r = channel(x * 16);
            let g = channel(y * 16);
            image::Rgb([r, g, 200])
        });
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(page)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    /// A work whose one page is a real, decodable image rather than the
    /// shared `work` fixture's ASCII bytes, and the temp directory it was
    /// built in.
    ///
    /// `work`'s pages are deliberately undecodable (see
    /// `cover.rs::bytes_that_are_not_an_image_fail_by_name`), which suits
    /// every route except one that actually has to generate a thumbnail. Named
    /// `w`/`a` — work id and edition key — to match the paths a thumb-route
    /// test addresses.
    ///
    /// Split out from `state_with_real_pages` for the one test that hands a
    /// `Vec<Work>` to `serve` rather than driving a built `ServeState`:
    /// `Work` is deliberately not `Clone` (see `works.rs`), so the work cannot
    /// be taken back out of an `Arc<ServeState>`, and a second copy of this
    /// fixture is the thing that goes stale.
    pub(crate) fn work_with_real_pages() -> (Work, std::path::PathBuf) {
        // A process id plus a monotonic per-process counter, not a fixed
        // name — the same pattern `cover.rs`'s `tempdir_for_test` uses, and
        // for the same reason. This helper used to open a single fixed
        // path and start by `remove_dir_all`-ing it with no cleanup after;
        // Rust runs tests in parallel threads, so the moment a second test
        // called this helper, whichever ran second would delete the first's
        // fixture out from under it mid-test.
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "taiyaku-test-real-pages-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join("a1.cbz")).unwrap());
        zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&real_page_bytes()).unwrap();
        zip.finish().unwrap();

        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [
              { "path": "a1.cbz", "sha256": "real", "pages": 1 }
            ], "spreads": [] }
          },
          "hub": "a",
          "alignments": []
        }"#;
        let path = root.join("taiyaku.json");
        std::fs::write(&path, json).unwrap();

        let work = Work::new(
            "w".into(),
            path,
            taiyaku_core::Sidecar::from_json(json).unwrap(),
        );
        (work, root)
    }

    /// The real-page work, wired to a `LocalLibrary` with its cache under the
    /// same temp directory.
    pub(crate) fn state_with_real_pages() -> std::sync::Arc<ServeState> {
        let (work, root) = work_with_real_pages();
        std::sync::Arc::new(ServeState {
            works: vec![work],
            source: std::sync::Arc::new(crate::LocalLibrary),
            cache_dir: root.join("covers"),
            log: None,
        })
    }

    /// The fixture work, but with its `"raw"` edition renamed to a
    /// caller-chosen key — for tests about the key itself, such as OPDS
    /// percent-encoding one that is not URL-safe. The archives on disk are
    /// unchanged; only the in-memory sidecar's edition map is renamed, which
    /// is all a router test reads.
    pub(crate) fn work_with_edition_key(dir: &str, key: &str) -> Work {
        let base = work(dir);
        let mut sidecar = (*base.sidecar()).clone();
        let edition = sidecar
            .editions
            .remove("raw")
            .expect("fixture defines the `raw` edition");
        sidecar.editions.insert(key.to_string(), edition);
        sidecar.hub = key.to_string();
        Work::new(base.id, base.path, sidecar)
    }

    pub(crate) fn cleanup(dir: &str) {
        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A request log writing to its own path under the temp dir.
    ///
    /// Named per test the way `work` is, so tests running in parallel cannot
    /// read each other's lines.
    pub(crate) fn log(name: &str) -> (std::sync::Arc<crate::Log>, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("{name}.jsonl"));
        let _ = std::fs::remove_file(&path);
        let log = std::sync::Arc::new(crate::Log::new(&path).unwrap());
        (log, path)
    }

    /// The lines a log wrote, parsed.
    ///
    /// Tests read the *file* rather than an in-memory copy, because the file
    /// is the only artefact anything outside this crate consumes —
    /// `scripts/request-summary.sh` reads it and nothing reads the other. A
    /// test that passes here therefore says something about what a person can
    /// actually measure after a reading session.
    ///
    /// Parsed as `Value` rather than back into `Entry` so the test is checking
    /// the wire format a `jq` script sees, not a round trip through the same
    /// struct definition that wrote it.
    pub(crate) fn logged_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("log line must parse as JSON: {e}: {line}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::StumpConfig;

    /// Minor 4 (whole-branch review): `UreqTransport::new` already trims a
    /// trailing slash for the transport; the resolution cache key has to
    /// agree, or `https://nas:10801/` and `https://nas:10801` are one
    /// instance to every request and two to the cache — silently discarding
    /// every prior resolution the first time someone happens to type the
    /// slash.
    #[test]
    fn a_trailing_slash_is_trimmed_once_at_construction() {
        let with_slash = StumpConfig::new("https://nas:10801/", "key");
        let without = StumpConfig::new("https://nas:10801", "key");

        assert_eq!(with_slash.base, without.base);
        assert_eq!(with_slash.base, "https://nas:10801");
    }

    /// A one-member work whose archive name is distinct from every other
    /// fixture's, so a `FakeTransport`'s recorded request bodies can tell
    /// which work triggered them — `crate::test_support::work` reuses
    /// `c1.cbz`/`c2.cbz` for every caller, which would make two such works
    /// indistinguishable in a GraphQL request body.
    fn distinct_work(dir: &str, member_name: &str) -> crate::Work {
        use std::io::Write;

        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let archive = format!("{member_name}.cbz");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(&archive)).unwrap());
        zip.start_file("p1.jpg", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"p1.jpg").unwrap();
        zip.finish().unwrap();

        let json = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "{member_name}", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "Raw", "members": [
                  {{ "path": "{archive}", "sha256": "{member_name}-sha", "pages": 1 }}
                ], "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": []
            }}"#
        );
        let path = root.join("taiyaku.json");
        std::fs::write(&path, &json).unwrap();

        crate::Work::new(
            member_name.to_string(),
            path,
            taiyaku_core::Sidecar::from_json(&json).unwrap(),
        )
    }

    /// `--refresh-stump` applies to every work it was handed, not just the
    /// first, and a work with nothing recorded is not a failure — the flag is
    /// typed once and most works in a library will have no cache file, so one
    /// refusing would be the worse answer.
    #[test]
    fn refreshing_forgets_every_work_and_tolerates_one_with_nothing_recorded() {
        let recorded = distinct_work("taiyaku-test-lib-forget-recorded", "recorded");
        let untouched = distinct_work("taiyaku-test-lib-forget-untouched", "untouched");
        let mut resolutions = crate::stump::cache::Resolutions::new();
        resolutions.insert("hash".into(), Some("book-1".into()));
        crate::stump::cache::store(&recorded, "http://nas:10801", &resolutions).unwrap();

        super::forget_all_works([&recorded, &untouched]);

        let gone = !crate::stump::cache::path_for(&recorded).exists();
        let still_absent = !crate::stump::cache::path_for(&untouched).exists();
        crate::test_support::cleanup("taiyaku-test-lib-forget-recorded");
        crate::test_support::cleanup("taiyaku-test-lib-forget-untouched");
        assert!(gone, "the recorded work's cache file must be removed");
        assert!(still_absent, "and the one that never had one is untouched");
    }

    /// The claim the loop makes in its own doc comment, which was correct by
    /// inspection only: one work whose file cannot be removed must not stop
    /// the works after it. A library is served from several sidecars and one
    /// read-only directory among them must not silently leave every later
    /// work still believing a stale resolution.
    ///
    /// Staged by taking write permission off one work's *directory*, since on
    /// Unix that is what governs deleting a file within it. Under root, or a
    /// filesystem ignoring the mode bits, that does not hold and the test
    /// would pass without measuring anything — so it says so and skips.
    #[test]
    fn one_work_that_cannot_be_forgotten_does_not_stop_the_others() {
        let stuck = distinct_work("taiyaku-test-lib-forget-stuck", "stuck");
        let after = distinct_work("taiyaku-test-lib-forget-after", "after");
        let mut resolutions = crate::stump::cache::Resolutions::new();
        resolutions.insert("hash".into(), Some("book-1".into()));
        for work in [&stuck, &after] {
            crate::stump::cache::store(work, "http://nas:10801", &resolutions).unwrap();
        }

        let stuck_dir = stuck.path.parent().unwrap().to_path_buf();
        let mut locked = std::fs::metadata(&stuck_dir).unwrap().permissions();
        locked.set_readonly(true);
        std::fs::set_permissions(&stuck_dir, locked).unwrap();
        let deletable_anyway = std::fs::remove_file(crate::stump::cache::path_for(&stuck)).is_ok();

        if deletable_anyway {
            let mut open = std::fs::metadata(&stuck_dir).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            open.set_readonly(false);
            let _ = std::fs::set_permissions(&stuck_dir, open);
            crate::test_support::cleanup("taiyaku-test-lib-forget-stuck");
            crate::test_support::cleanup("taiyaku-test-lib-forget-after");
            eprintln!(
                "NOT MEASURED: one_work_that_cannot_be_forgotten_does_not_stop_the_others \
                 (a read-only directory still allows deletion here)"
            );
            return;
        }

        super::forget_all_works([&stuck, &after]);

        let stuck_remains = crate::stump::cache::path_for(&stuck).exists();
        let later_forgotten = !crate::stump::cache::path_for(&after).exists();
        // Restore before cleanup: `remove_dir_all` needs to unlink the files
        // inside, which the locked directory forbids.
        let mut open = std::fs::metadata(&stuck_dir).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        open.set_readonly(false);
        let _ = std::fs::set_permissions(&stuck_dir, open);
        crate::test_support::cleanup("taiyaku-test-lib-forget-stuck");
        crate::test_support::cleanup("taiyaku-test-lib-forget-after");

        assert!(
            stuck_remains,
            "the precondition: this work's file genuinely could not be removed"
        );
        assert!(
            later_forgotten,
            "a work after the failure must still be forgotten"
        );
    }

    /// O8: resolution reports each work as it finishes rather than handing
    /// back one aggregate at the end.
    ///
    /// Binding the listener first is only half the fix. If every work's
    /// answers arrived together, a twenty-work library would still serve all
    /// twenty from disk until the twentieth had been asked about — the wait
    /// moved behind the listener rather than removed. Reporting per work makes
    /// the first work usable while the last is still being resolved.
    #[test]
    fn each_work_is_learned_as_it_finishes_rather_than_all_at_the_end() {
        use crate::stump::http::test_support::FakeTransport;

        let first = distinct_work("taiyaku-test-lib-progressive-first", "alpha");
        let second = distinct_work("taiyaku-test-lib-progressive-second", "bravo");
        // Stump answers, and what it answers is "no such book" — an answer
        // rather than an outage, so the breaker does not fire and both works
        // are asked about.
        let fake =
            FakeTransport::new().on_post("/api/graphql", r#"{"data":{"media":{"nodes":[]}}}"#);

        let mut batches: Vec<Vec<String>> = Vec::new();
        super::resolve_all_works(
            &fake,
            &[first, second],
            "http://nas:10801",
            &mut |learned| {
                let mut keys: Vec<String> = learned.keys().cloned().collect();
                keys.sort();
                batches.push(keys);
            },
        );

        crate::test_support::cleanup("taiyaku-test-lib-progressive-first");
        crate::test_support::cleanup("taiyaku-test-lib-progressive-second");
        assert_eq!(
            batches.len(),
            2,
            "one report per work, not one at the end: {batches:?}"
        );
        assert_eq!(
            batches[0],
            vec!["alpha-sha".to_string()],
            "the first work's answers must arrive before the second is asked about"
        );
        assert_eq!(batches[1], vec!["bravo-sha".to_string()]);
    }

    /// Important 3 (whole-branch review): the circuit breaker must span
    /// works, not stop at the boundary of the first one. Against an
    /// unreachable instance, the second work's member must never be asked
    /// about at all — proven by asserting on the fake's recorded requests,
    /// not merely on what was learned.
    #[test]
    fn resolve_all_works_stops_asking_after_the_first_transport_failure() {
        use crate::stump::http::test_support::FakeTransport;

        let first = distinct_work("taiyaku-test-lib-resolve-all-first", "alpha");
        let second = distinct_work("taiyaku-test-lib-resolve-all-second", "bravo");
        // No handlers registered, so every POST is a transport-level error —
        // standing in for an unreachable instance, the convention every
        // other test in this crate uses.
        let unreachable = FakeTransport::new();

        let mut resolutions = crate::stump::cache::Resolutions::new();
        super::resolve_all_works(
            &unreachable,
            &[first, second],
            "http://nas:10801",
            &mut |learned| resolutions.extend(learned),
        );

        let requests = unreachable.requests();
        crate::test_support::cleanup("taiyaku-test-lib-resolve-all-first");
        crate::test_support::cleanup("taiyaku-test-lib-resolve-all-second");
        assert!(
            resolutions.is_empty(),
            "nothing was resolved: {resolutions:?}"
        );
        assert_eq!(
            requests.len(),
            1,
            "only the first work's first (failing) member should be attempted, not one \
             lookup per work: {requests:?}"
        );
        assert!(
            !requests.iter().any(|r| r.contains("bravo")),
            "the second work must never be asked about once the first aborted by \
             transport failure: {requests:?}"
        );
    }

    /// The tile cache is a *sibling* of the cover cache, not the same
    /// directory and not a child of it — the arrangement `thumb_cache_dir`'s
    /// own doc comment argues for, on the grounds that only one of the two is
    /// cheap to rebuild, and which nothing checked. `thumb_cache_dir` could
    /// return `PathBuf::default()`, the empty path, and every tile in every
    /// library would share one directory in the process's working directory.
    #[test]
    fn the_tile_cache_is_a_sibling_of_the_cover_cache() {
        let state = crate::test_support::state("taiyaku-test-thumb-dir");
        let covers = state.cache_dir.clone();
        let thumbs = state.thumb_cache_dir();

        crate::test_support::cleanup("taiyaku-test-thumb-dir");
        assert_ne!(thumbs, covers, "not the same directory");
        assert_eq!(
            thumbs.parent(),
            covers.parent(),
            "siblings: same parent, different leaf"
        );
        assert_eq!(thumbs.file_name().unwrap(), "thumbs");
        assert!(
            !thumbs.as_os_str().is_empty(),
            "an empty path would put every library's tiles in one place"
        );
    }

    /// The sibling invariant above, re-asserted at a *resolved* root rather
    /// than assumed to survive the move off the relative path. The two are
    /// different claims: that one holds `cache_dir` fixed from the fixture,
    /// this one takes whatever `cache_root` produced and checks the tiles
    /// still land beside the covers and inside the application's directory.
    ///
    /// Minor 7 (whole-branch review): it used to build the shared two-member
    /// cbz fixture on disk — four zip entries and a sidecar — to assert a
    /// derivation that touches no file at all. The work is what the claim
    /// never needed, so it is an empty `Vec` here; the archives are still
    /// built, and the paths still exercised against real bytes, by
    /// `serve_caches_covers_and_tiles_under_the_resolved_root` below.
    #[test]
    fn a_resolved_root_keeps_covers_and_thumbs_as_siblings() {
        let state = crate::ServeState {
            works: Vec::new(),
            source: std::sync::Arc::new(crate::LocalLibrary),
            cache_dir: crate::cache_root::cache_root(
                None,
                Some(std::ffi::OsStr::new("/var/cache/fixture")),
                None,
            )
            .covers(),
            log: None,
        };
        let covers = state.cache_dir.clone();
        let thumbs = state.thumb_cache_dir();

        assert_eq!(
            covers,
            std::path::PathBuf::from("/var/cache/fixture/taiyaku/covers")
        );
        assert_eq!(
            thumbs,
            std::path::PathBuf::from("/var/cache/fixture/taiyaku/thumbs"),
            "tiles land beside the covers, inside the application's own directory"
        );
        assert_eq!(covers.parent(), thumbs.parent(), "siblings");
    }

    /// Aborting on the *last* work must say nothing: there is nothing left to
    /// skip, and a sentence promising to skip "0 work(s)" — or, with the
    /// subtraction mutated, some other number of works that do not exist — is
    /// worse than silence.
    ///
    /// The abort test above asserts which requests were made, which is the
    /// right thing for it to assert and cannot see any of this.
    #[test]
    fn the_skipping_notice_counts_only_the_works_actually_left() {
        assert_eq!(
            crate::skipping_notice(1, 0),
            None,
            "gave up on the only work: nothing follows it"
        );
        assert_eq!(
            crate::skipping_notice(3, 2),
            None,
            "gave up on the last of three: nothing follows it either"
        );

        let two_left = crate::skipping_notice(3, 0).expect("two works follow the first");
        assert!(
            two_left.contains("remaining 2 work(s)"),
            "must count what is left, not what there was: {two_left}"
        );
        let one_left = crate::skipping_notice(3, 1).expect("one work follows the second");
        assert!(one_left.contains("remaining 1 work(s)"), "{one_left}");
    }

    /// `serve` actually binds and answers.
    ///
    /// The one thing in this crate that opens a socket, and so — by the design
    /// its own doc comment states — the one thing every other test avoids
    /// driving. The cost of that is that `serve` could be replaced wholesale by
    /// `Ok(())`, starting no server at all, with every other test in the crate
    /// at the time green. A composition root nothing composes is worth exactly
    /// one test — two, since
    /// `serve_caches_covers_and_tiles_under_the_resolved_root` below found the
    /// second thing about this function that only a socket can observe.
    ///
    /// The port is taken by binding and releasing, which is racy in principle;
    /// the poll below is what makes it deterministic in practice, and a genuine
    /// collision fails loudly rather than passing wrongly.
    ///
    /// **`multi_thread` is required, not stylistic.** `ureq` is a blocking
    /// client — deliberately, per `stump::http`'s doc — so polling with it on
    /// the default current-thread runtime blocks the very runtime that is
    /// supposed to be driving `serve`, and the test deadlocks rather than
    /// failing. Written the obvious way first, and it hung.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_binds_the_address_and_answers_on_it() {
        let works = vec![crate::test_support::work("taiyaku-test-serve-bind")];

        let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let addr = probe.local_addr().unwrap().to_string();
        drop(probe);

        // The cache root is pinned by the flag rather than left to resolve,
        // so this test neither reads the environment it happens to run under
        // nor can write a cache into a real `$HOME`. It lives under the
        // fixture's own temp directory, which `cleanup` below removes.
        let cache = std::env::temp_dir()
            .join("taiyaku-test-serve-bind")
            .join("cache");

        let serving = tokio::spawn({
            let addr = addr.clone();
            async move { crate::serve(works, &addr, None, None, false, Some(cache)).await }
        });

        // Poll rather than sleep a fixed time: the bind is the thing under
        // test, so waiting for it is the assertion.
        let url = format!("http://{addr}/api/works");
        let mut answered = None;
        for _ in 0..100 {
            if serving.is_finished() {
                break;
            }
            if let Ok(mut r) = ureq::get(&url).call() {
                answered = Some(r.body_mut().read_to_string().unwrap_or_default());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        serving.abort();
        crate::test_support::cleanup("taiyaku-test-serve-bind");

        let body = answered.expect("serve must bind the address and answer on it");
        assert!(
            body.contains("\"t\""),
            "and answer with the works it was given: {body}"
        );
    }

    /// Important 2 (whole-branch review): `serve` builds its `ServeState` from
    /// the *resolved* root, and generated files really land under it.
    ///
    /// This is the whole subject of the branch, and until this test nothing
    /// observed it. Replacing `cache_dir: resolved.covers()` with the old
    /// `PathBuf::from(".taiyaku-cache/covers")` — leaving `resolved` in place
    /// so the notice still compiles — leaves every other test in the workspace
    /// green, and this one is the only failure: the sibling tests assign
    /// `cache_dir` themselves, and the bind test asks for `/api/works`, which
    /// writes nothing. Confirmed by making that mutation rather than by
    /// reasoning about it.
    ///
    /// So it goes through a socket rather than a `oneshot` router. `serve` is
    /// where the resolution meets the state, and a test that builds the state
    /// itself cannot see that line at all.
    ///
    /// **Both leaves, not just the covers.** A cover exercises `cache_dir`
    /// directly and a tile exercises `thumb_cache_dir`, which is derived from
    /// it — so this also pins the sibling arrangement against a root that was
    /// resolved rather than typed into a fixture, with real bytes on both
    /// sides.
    ///
    /// The fixture's pages are real PNGs (`work_with_real_pages`) because both
    /// routes decode: the shared two-member fixture's "jpegs" are their own
    /// filenames, and every request here would 404 on the decode instead of
    /// writing anything.
    ///
    /// It also carries Important 5's other half — that `serve` *creates* the
    /// root rather than leaving the first request to — which is checkable only
    /// from here, before any request that would create it in passing.
    ///
    /// `multi_thread` and the poll are `serve_binds_the_address_and_answers_on_it`'s,
    /// for its reasons.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_caches_covers_and_tiles_under_the_resolved_root() {
        let (work, fixture) = crate::test_support::work_with_real_pages();
        let sha = work
            .sidecar()
            .editions
            .get("a")
            .expect("the fixture's one edition")
            .members[0]
            .sha256
            .clone();

        let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let addr = probe.local_addr().unwrap().to_string();
        drop(probe);

        // Pinned by the flag under the fixture's own temp directory: the test
        // must not read the environment it happens to run under, and must not
        // be able to write into a real `$HOME`.
        let cache = fixture.join("cache");
        let serving = tokio::spawn({
            let addr = addr.clone();
            let cache = cache.clone();
            async move { crate::serve(vec![work], &addr, None, None, false, Some(cache)).await }
        });

        let mut bound = false;
        for _ in 0..100 {
            if serving.is_finished() {
                break;
            }
            if ureq::get(&format!("http://{addr}/api/works"))
                .call()
                .is_ok()
            {
                bound = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        // Read *before* any request that would create it lazily, which is what
        // makes it an observation of `cache_root::create` being called at all
        // rather than of `cover.rs` making the directory on its way past.
        // `serve` binds after it creates, so an answered request means it ran.
        let created_at_startup = cache.join("covers").is_dir();

        // `ureq` reports a non-2xx as an `Err` by default (see
        // `stump::http::get_bytes`, which turns that off deliberately), so the
        // status is carried as the message a failure prints rather than
        // unwrapped here.
        let cover = ureq::get(&format!("http://{addr}/opds/v1.2/works/w/a/cover"))
            .call()
            .map(|r| r.status().as_u16())
            .map_err(|e| e.to_string());
        let tile = ureq::get(&format!("http://{addr}/api/works/w/a/pages/0/thumb"))
            .call()
            .map(|r| r.status().as_u16())
            .map_err(|e| e.to_string());

        // Read before the fixture is removed, and before the assertions, so a
        // failing assertion still leaves no server running and no temp tree.
        let expected_cover = crate::cover::cache_path(&cache.join("covers"), &sha);
        let expected_tile = crate::cover::thumb_cache_path(&cache.join("thumbs"), &sha, 0);
        let cover_cached = expected_cover.is_file();
        let tile_cached = expected_tile.is_file();

        serving.abort();
        let _ = std::fs::remove_dir_all(&fixture);

        assert!(bound, "serve must bind before anything else is asserted");
        assert!(
            created_at_startup,
            "Important 5: the cache root is created at startup, not by whichever request \
             first happens to want it: {}",
            cache.join("covers").display()
        );
        assert_eq!(cover, Ok(200), "the cover route must have answered");
        assert_eq!(tile, Ok(200), "the tile route must have answered");
        assert!(
            cover_cached,
            "the generated cover must be written under the root `serve` resolved, not \
             under a path baked into the state: {}",
            expected_cover.display()
        );
        assert!(
            tile_cached,
            "and the tile beside it, in the sibling `thumbs` under the same root: {}",
            expected_tile.display()
        );
    }
}

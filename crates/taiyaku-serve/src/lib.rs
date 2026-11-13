//! `taiyaku serve` — the sidecar, its pages, and where you were, over HTTP.
//!
//! Everything above `PageSource` is indifferent to where pages come from, which
//! is what lets taiyaku run beside Stump or replace it without a
//! redesign. See `docs/superpowers/specs/2026-07-30-reader-design.md`.

mod about;
mod alignments;
mod auth;
mod backup_api;
mod cache_root;
mod cache_sweep;
mod cbz;
mod composite;
/// Spec C4 §6's rule-editor routes: the admin-only CRUD, preview and import
/// over [`taiyaku_library::rules`] and [`taiyaku_library::preview`].
///
/// Private, for the reason `curation`'s own doc comment gives: every handler
/// here is reached through [`build_router`] and nothing else.
mod config_api;
#[cfg(test)]
mod corpus;
mod cover;
/// Spec C3 §7's proposals inbox: the admin-only routes over
/// [`taiyaku_library::proposals`].
///
/// Private, because every handler in it is reached through
/// [`build_router`] and nothing else — a `pub` here would be surface with no
/// caller.
mod curation;
mod dirs_api;
pub(crate) mod download;
mod libraries_api;
pub mod library_api;
mod library_job;
mod limit;
mod metadata;
/// A work's notes, beside its sidecar.
///
/// Public where `progress` is private, and deliberately: `progress::load` and
/// `progress::store` are reachable from the crate root as `Progress` plus two
/// routes, but `load`/`store`/`to_json` re-exported there would be three names
/// too vague to sit beside `load_works`. The module path says what they load.
pub mod notes;
mod opds;
mod progress;
mod registry;
mod reqlog;
mod routes;
mod scan_jobs;
mod scheduler;
pub mod session;
mod source;
mod stamp;
mod stump;
mod truth;
mod users_api;
mod view;
mod web;
mod works;

use std::io::Write as _;
use std::sync::Arc;

pub use composite::{CompositeSource, StumpSide};
pub use library_job::{JobGuard, LibraryJob};
pub use registry::{Refresh, Snapshot, Works};
pub use reqlog::{Entry, Log};
pub use routes::build_router;
pub use source::{LocalLibrary, PageSource, ServedPage};
// `Progress` lives in `taiyaku-library`, where the `state` module needs it;
// re-exported here so `crate::Progress` resolves.
pub use taiyaku_library::Progress;
// `from_library` projects every work in a library once, and refuses the lot if
// any one will not project. `taiyaku serve` does not run it: with no paths it
// builds a live `Works` through `Works::library`, which reads the same rows but
// leaves out and names a work that will not project. It is for the fixtures
// that build a fixed registry over a library.
//
// What it decides about a work whose `sidecar_path` is NULL: the path becomes
// empty, `Work::has_document` reads that as "no document", and the alignment
// route writes rows without a file.
pub use works::{Work, from_library, load_works, work_id};

/// Everything a request needs: which works exist, and where pages come from.
#[derive(Debug)]
pub struct ServeState {
    /// Where the progress, notes and truth routes read and write: the library
    /// `works` below was projected from or imported into (`taiyaku-cli`'s
    /// `serve` imports every sidecar path it was handed through
    /// `library::import_canonical`). So a bridge row `works::row_id_for_write`
    /// mints lands on the database a later `taiyaku library import`
    /// reconciles against, not one that vanishes with the process. Only a
    /// fixed registry mints one, for a work that bypassed import as
    /// `test_support::state`'s fixtures do; a live registry never does (O184).
    pub library: Arc<taiyaku_library::Library>,
    /// The works this process serves through `/api/works`, the reader's
    /// routes and OPDS: a [`Works`] registry, live when the server was
    /// started with no paths and fixed when it was handed sidecar paths.
    ///
    /// **Look a work up with [`Self::find`] on a runtime worker and with
    /// [`Self::find_blocking`] on the blocking pool.** A miss in a live
    /// registry asks the library whether it has moved before answering "no
    /// work", and that question reads the database.
    pub works: Works,
    pub source: Arc<dyn PageSource>,
    /// Where generated covers are kept between runs.
    pub cache_dir: std::path::PathBuf,
    /// Set when `--log-requests` was given; otherwise nothing is recorded.
    pub log: Option<Arc<crate::reqlog::Log>>,
    /// What this server knows about the deployment it sits in: whether the
    /// session cookie carries `Secure`, whether the claim route exists, and
    /// which proxies it believes. It also carries the permits this server's
    /// password verifies run under.
    ///
    /// Spec A2 §6 and §5. Small because §11 puts TLS in a reverse proxy, so
    /// none of that is here.
    pub auth: crate::session::AuthConfig,
    /// Every root scan running or just finished, by root id: the registry for
    /// `POST`/`GET /api/library/roots/{id}/scan`, a library's scan (an entry
    /// for each directory it walks) and the scheduler's (each one a tick takes).
    ///
    /// **In-process only, and deliberately.** Nothing here survives a
    /// restart: a process that died mid-scan left no lock behind, and the
    /// next scan is a full one regardless of what the last one reached, so
    /// there is nothing a persisted entry would buy that a fresh `HashMap`
    /// does not already give for free. The registry's whole job is refusing
    /// a *second* start against a root already scanning in *this* process —
    /// C4 §3 — not remembering a scan across runs; `root.last_scan_at`/
    /// `last_scan_status` (`taiyaku_library::scan::scan_root`) are the
    /// durable record, written on every outcome including the refusal.
    ///
    /// A `Mutex<HashMap>` rather than a `DashMap` or similar: entries are
    /// read and written under a lock held only long enough to check, insert
    /// or replace the keys of one scan, never across the scan itself —
    /// `start_scan` drops the guard before `tokio::task::spawn_blocking`
    /// ever begins walking a filesystem, and the spawned task reacquires it
    /// only to write its finished report back. A crate this size gains
    /// nothing from a lock-free map for that shape of access.
    pub scans: Arc<std::sync::Mutex<std::collections::HashMap<i64, ScanProgress>>>,
    /// The store's job lock: [`LibraryJob`] names the jobs that take it, a
    /// scan's derive and a save's `preview::apply` among them.
    ///
    /// **Why a second lock rather than teaching [`Self::scans`] the
    /// question.** `scans` answers "is *this root* already scanning," keyed
    /// by root id, which a save is not; turning it into "is *anything*
    /// running" by scanning its values under the same `Mutex` would still
    /// leave a gap between that check and the moment a save actually calls
    /// `working` for a scan to start in — a check-then-act race, not a fix.
    /// A `Mutex<()>` closes that gap by construction: acquiring it and
    /// running the long job are the same critical section.
    ///
    /// **What this closes.** `scan_jobs::run`'s own scan derives one work at a
    /// time — the right grain for a scan, so a half-finished one still leaves
    /// most of the library current. `taiyaku_library::preview::apply` instead
    /// holds one `BEGIN IMMEDIATE` across a whole-library derive so a save
    /// lands atomically. Without this lock nothing would serialise those two
    /// against each other: a save's single transaction could land between two
    /// of a scan's own, replacing the rule table and re-deriving the whole
    /// library while the scan's already-compiled `Engine` is mid-pass under the
    /// rules that transaction just replaced; the scan would then write its
    /// remaining works back under rules the `rule` table does not name.
    ///
    /// **Acquired and released entirely inside `spawn_blocking`, never held
    /// across an `.await`.** Both a save and a scan already run on a
    /// blocking-pool thread with nothing else to yield to for the whole
    /// call, so holding a `std::sync::Mutex` for the length of a derivation
    /// costs that one thread, not the async runtime.
    ///
    /// **A save does not wait on this lock; it answers `409` at once if
    /// anything already holds it** ([`LibraryJob`] names the jobs that take
    /// it), as [`Self::scans`] answers a second scan of one root: a caller
    /// learns why its save did not land rather than waiting on whatever runs.
    /// A scan blocks instead: it already runs unwatched in the background.
    /// **This lock is one process's.** Across processes the derive lease
    /// (`taiyaku_library::lease`) serialises whole-store derives: a scan waits
    /// for it and a save or a stamp tries it once, as each does this lock (O51).
    pub library_job: Arc<LibraryJob>,
    /// The directories `GET /api/library/dirs` may list beneath:
    /// `--dirs`, or `TAIYAKU_DIRS`. Empty lists nothing.
    pub dirs: Vec<std::path::PathBuf>,
}

/// One root's scan, as the registry in [`ServeState::scans`] tracks it —
/// [`crate::config_api::scan_status`]'s own source of truth while a scan is
/// still running, and the last word on how it went once `done` flips.
///
/// **Survives exactly as long as the process does, and exactly one scan per
/// root at a time** — see [`ServeState::scans`]'s own doc comment for why
/// neither is a shortcoming here.
#[derive(Debug)]
pub struct ScanProgress {
    /// When this scan was started — [`std::time::Instant`], not a wall-clock
    /// timestamp, since the only thing anything reads this for is "how long
    /// has this been running," never "what time did it start." Wall-clock
    /// time already has a durable home: `root.last_scan_at`, written by
    /// `scan_root` itself once the scan finishes.
    pub started: std::time::Instant,
    /// Whether the spawned task has finished — successfully, or by failing —
    /// and written [`Self::report`]. A caller polling
    /// `GET /api/library/roots/{id}/scan` reads this to know whether what it
    /// is looking at is a live-progress snapshot or the finished answer.
    pub done: bool,
    /// The rendered report, once `done` is true — `null`/`None` while a scan
    /// is still running, since there is nothing to report yet beyond the
    /// fact that one is in flight.
    ///
    /// Whole [`taiyaku_library::scan::ScanReport`] and
    /// [`taiyaku_library::derive::DeriveReport`] structures, rendered with
    /// every field present — including the empty collections — per C4 §3's
    /// discipline that a scan names what it did not do rather than only what
    /// it did; see [`crate::config_api::scan_status`]'s own doc comment.
    pub report: Option<serde_json::Value>,
    /// The library whose request reserved this slot: the library a Scan
    /// library named, or for a Scan directory the library its page named,
    /// else the directory's one library (decision 27). A library's status
    /// names a scan by it, since a scan of one library walks directories of
    /// others. `None` for a shared directory's scan named by no page, and for
    /// a slot no request reserved.
    pub library: Option<i64>,
    /// Whether this scan waits (O51): to start, for the job lock another job
    /// holds, set on every directory reserved; or to derive, for the derive
    /// lease another derive holds, set on the directories whose walks recorded
    /// something. Their status reads `waiting` from this as well as the store
    /// ([`scan_jobs::status_sent`]): a holder in this process may hold the
    /// store's write lock for its whole derive, and the store's word with it.
    pub waiting: bool,
}

impl ServeState {
    /// Where generated filmstrip tiles are kept between runs.
    ///
    /// A sibling of `cache_dir` rather than the same directory, so that the two
    /// can be thrown away separately (O11) — because only one of them is cheap
    /// to rebuild. The covers are small; the tiles and reading-size pages
    /// beside them are large and slow to regenerate. One shared directory would
    /// make clearing the cheap half impossible without taking the expensive
    /// half with it, which is exactly the bluntness this arrangement avoids.
    #[must_use]
    pub fn thumb_cache_dir(&self) -> std::path::PathBuf {
        self.cache_dir.with_file_name("thumbs")
    }

    /// The work a route names, for code running on a runtime worker.
    ///
    /// A hit is a read lock held long enough to clone an `Arc`. A miss in a
    /// live registry first asks, on the blocking pool and never here,
    /// whether the library has moved since its works were last read, and
    /// reads them again if it has. So a volume a scan has just derived opens
    /// on the first request for it rather than after the next refresh tick.
    ///
    /// **One wording for a work that does not exist**, shared with
    /// [`Self::find_blocking`]. Every route that is addressed by work
    /// answers it the same way.
    ///
    /// # Errors
    ///
    /// Returns an error, worded `` no work `{id}` ``, if `id` names no work
    /// this server serves, after that refresh for a live registry.
    pub async fn find(self: &Arc<Self>, id: &str) -> Result<Arc<Work>, String> {
        if let Some(found) = self.works.get(id) {
            return Ok(found);
        }
        if self.works.is_live() {
            let asking = Arc::clone(self);
            // The refresh reports itself, in its own order (`Works::refresh`).
            if let Err(panicked) =
                tokio::task::spawn_blocking(move || asking.works.refresh_if_moved(&asking.library))
                    .await
            {
                // Not `eprintln!`, which panics when nothing reads stderr,
                // and would fail this request over a line nobody can read.
                let _ = writeln!(
                    std::io::stderr(),
                    "taiyaku: the refresh panicked: {panicked}"
                );
            }
            if let Some(found) = self.works.get(id) {
                return Ok(found);
            }
        }
        Err(no_work(id))
    }

    /// [`Self::find`], for code already on the blocking pool.
    ///
    /// **Reads the database on a miss**, so call it only where blocking is
    /// allowed: inside `spawn_blocking`, or a test.
    ///
    /// # Errors
    ///
    /// As [`Self::find`].
    pub fn find_blocking(&self, id: &str) -> Result<Arc<Work>, String> {
        if let Some(found) = self.works.get(id) {
            return Ok(found);
        }
        if self.works.is_live() {
            // The refresh reports itself, in its own order (`Works::refresh`).
            let _ = self.works.refresh_if_moved(&self.library);
            if let Some(found) = self.works.get(id) {
                return Ok(found);
            }
        }
        Err(no_work(id))
    }

    /// One page of `view`'s edition, by its full index in that edition's flat
    /// sequence, located in the sidecar `view` checked
    /// ([`crate::view::EditionView::locate`]).
    ///
    /// Every source and every route — the API, the OPDS stream and the cover
    /// — arrives here. The failure messages name what was missing, for the
    /// server's log; a route sends `routes::unreadable`'s sentence instead,
    /// since a message can name a path on the server (O267).
    ///
    /// # Errors
    ///
    /// Returns an error if the edition is not in that sidecar, if `index` is
    /// at or past its total page count (the message then states how many
    /// pages it has), or if the underlying source (`PageSource::page`) fails
    /// to produce the page it located — a local read failure, or, over Stump,
    /// a failed fetch. The source's failure appends the work's path, for the
    /// log.
    ///
    /// **Every route answers an `Err` from this function with a `404`, never
    /// a `500`.** A local read failure that has nothing to do with what the
    /// client asked for — a permission refusal, a corrupted archive — is
    /// therefore answered as a page the client cannot have, named without why.
    pub(crate) fn page(
        &self,
        view: &crate::view::EditionView,
        index: u32,
    ) -> Result<ServedPage, String> {
        let (member, within) = view.locate(index)?;
        let work = view.work();
        let root = work.path.parent().unwrap_or(std::path::Path::new("."));
        self.source
            .page(root, member, within)
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
    /// Left to each consumer, the transport's `base` (`UreqTransport::new`)
    /// and `resolve_work`'s cache key could disagree: `https://nas:10801/` and
    /// `https://nas:10801` would be one instance to every request and two to
    /// the resolution cache, so typing the slash once would discard every
    /// prior resolution.
    pub fn new(base: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            key: key.into(),
        }
    }
}

/// How [`serve`] runs, beyond the library and the works it serves.
///
/// **A struct because `serve` reached its limit on positional arguments.**
#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// `host:port` to listen on.
    pub addr: String,
    /// A Stump instance to read pages from, if one is configured.
    pub stump: Option<StumpConfig>,
    /// Where `--log-requests` writes, if it was given.
    pub log_requests: Option<std::path::PathBuf>,
    /// Throw away every remembered Stump resolution before resolving again.
    pub refresh_stump: bool,
    /// `--cache`: the cache root, overriding `$XDG_CACHE_HOME/taiyaku`.
    pub cache: Option<std::path::PathBuf>,
    /// What this server knows about the deployment in front of it.
    pub auth: crate::session::AuthConfig,
    /// A built web app (`web/dist`) to serve beside the API — see
    /// [`crate::web`]. `None` serves the API alone, which is what `vite dev`'s
    /// proxy expects.
    pub web: Option<std::path::PathBuf>,
    /// `--dirs`: what the directory browser may list beneath.
    pub dirs: Vec<std::path::PathBuf>,
    /// The cache cap in bytes, 0 for no cap. It holds at each sweep, which
    /// deletes the least recently used renderings until the cache is at or
    /// under it; between sweeps the cache may grow past it.
    pub cache_max: u64,
}

/// Says, line by line to `say`, why each directory the server was started with
/// lists nothing. Split from `serve` so a test can read what it says.
pub(crate) fn announce_dirs(dirs: &[std::path::PathBuf], say: &mut dyn FnMut(&str)) {
    for line in dirs_api::notices(dirs) {
        say(&line);
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
/// it before calling this. The tests in this crate that build a `ServeState`
/// without going through an import (`test_support::state`, most of this file's
/// own fixtures) pass an empty `Library::open_in_memory()`: this parameter
/// only says *which* library to serve out of, not that one must hold every
/// work already.
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
/// Everything but the library and the works arrives as [`ServeOptions`]; the
/// next option belongs there too.
pub async fn serve(
    library: Arc<taiyaku_library::Library>,
    works: Works,
    options: ServeOptions,
) -> Result<(), String> {
    let ServeOptions {
        addr,
        stump,
        log_requests,
        refresh_stump,
        cache,
        auth,
        web,
        dirs,
        cache_max,
    } = options;
    let addr = addr.as_str();
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
                let held = works.snapshot();
                forget_all_works(held.iter().map(|work| &**work));
            }
            let transport: Arc<dyn stump::http::Transport> = Arc::new(
                stump::http::UreqTransport::new(config.base.clone(), config.key),
            );
            // O8: constructed with **nothing resolved**. Resolving here, before
            // the bind, would hold the listener back for one GraphQL round trip
            // per member on a cold start against a reachable instance, and for
            // a full 30 s POST timeout on every start against an unreachable
            // one, to reach pages that sit on local disk the whole time.
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
    // The rule of the `--refresh-stump` notice, applied to a default, not a
    // flag: a thing that quietly does something surprising is worse than one
    // that says so. The fallback branch is the only one that follows the
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

    announce_dirs(&dirs, &mut |line| eprintln!("{line}"));

    let state = Arc::new(ServeState {
        library,
        works,
        source,
        cache_dir: resolved.covers(),
        log,
        auth,
        scans: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        library_job: Arc::default(),
        dirs,
    });

    // §4's other half: expired sessions are deleted lazily on lookup, and swept
    // once here. Neither is enough alone — a server that runs for a month never
    // reaches this line again, and a row nobody presents is never looked up.
    // Reported rather than fatal: a library that cannot be swept can still be
    // read, and refusing to start over housekeeping would be the worse trade.
    // §5: "It logs loudly at startup and on every request it serves, because a
    // flag whose danger is invisible while it is doing nothing is a flag that
    // stays on." Both halves matter — a person who left it on a month ago will
    // not reread the startup line, and a person who set it today will not see
    // the per-request one until somebody uses it.
    // **Said whether or not the flag is on.** A fresh or upgraded library has
    // no account anybody can authenticate as — migration 4 rewrites the seeded
    // row's NULL password to the locked placeholder — so `serve` would bind,
    // print "listening on…", and answer 401 to every route with nothing
    // anywhere saying why.
    // **`is_claimed`, not `admin_exists`** — the message says "no account with
    // a password" and that is literally what `is_claimed` counts. Read through
    // `admin_exists` it would also fire on a library where an ordinary reader
    // could log in perfectly well, telling its owner every route would answer
    // 401 while they were using it. Same confusion as the claim route's.
    if !state.auth.allow_web_setup
        && taiyaku_library::account::is_claimed(&state.library.conn()).is_ok_and(|yes| !yes)
    {
        eprintln!(
            "WARNING: this library has no account with a password, so every route will \
             answer 401. Run `taiyaku library user promote <name>` against the same \
             --db to set one — it promotes the account this library already has, so \
             every note and reading position already recorded belongs to it."
        );
    }
    if state.auth.allow_web_setup {
        // The gate this notice describes is the claim route's own, so it reads
        // `is_claimed` too. Reading `admin_exists` here would print "ANYONE can
        // claim it" on a library the route refuses — a warning that cries wolf
        // is a warning that gets ignored on the day it is right.
        let claimed = taiyaku_library::account::is_claimed(&state.library.conn());
        match claimed {
            Ok(true) => eprintln!(
                "note: --allow-web-setup is on but this library has already been \
                 set up, so the claim route is shut. The flag can be removed."
            ),
            _ => eprintln!(
                "WARNING: --allow-web-setup is on and no account in this library \
                 has a password. ANYONE who can reach this server can claim it. It \
                 shuts by itself the moment any account has one — or run \
                 `taiyaku library user promote` instead and start without the flag."
            ),
        }
    }

    match taiyaku_library::account::sweep_expired_sessions(&state.library.conn()) {
        Ok(0) => {}
        Ok(swept) => eprintln!("swept {swept} expired session(s)"),
        Err(complaint) => eprintln!("could not sweep expired sessions: {complaint}"),
    }
    // The rate limiter's own windows, which nothing else removes: a successful
    // login clears one exact key, so every other row would stay for good and a
    // loop of logins for random names would be an unauthenticated way to grow
    // the shared database. Beside the sessions because it is the same kind of
    // housekeeping and has the same "reported, never fatal" answer.
    match taiyaku_library::account::sweep_lapsed_attempts(&state.library.conn()) {
        Ok(0) => {}
        Ok(swept) => eprintln!("swept {swept} lapsed login-attempt window(s)"),
        Err(complaint) => eprintln!("could not sweep lapsed login attempts: {complaint}"),
    }
    // O186, before the bind: only this server writes `deriving` and `waiting`,
    // and none of its own scans has begun, so each one now is a scan that a
    // stop, a panic or a refused write (O272) left without its last word.
    scheduler::interrupt_cut_derives(&state.library);
    // Before the bind, so the `listening` line below is never printed while a
    // signal would still kill the process outright.
    let stop = shutdown_signal();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("{addr}: {e}"))?;
    eprintln!("taiyaku serve listening on http://{addr}");

    // O8: after the bind, deliberately — including the order of these two
    // lines on stderr, since "listening" arriving before the resolution
    // chatter is how a person watching a cold start can see that the wait is
    // not theirs. `spawn_blocking` because resolution is a network
    // round trip *and* a local page hash per member, neither of which belongs
    // on an async worker; `get_page` and the OPDS handlers already keep to
    // that boundary for the same reason (O12).
    if let Some((transport, side, base)) = pending {
        let resolving = Arc::clone(&state);
        tokio::task::spawn_blocking(move || {
            // The works present at startup, and only those. Stump survives only as
            // a migration path (the store spec's §10), so a work a later scan adds
            // is served from disk.
            let held = resolving.works.snapshot();
            resolve_all_works(transport.as_ref(), &held[..], &base, &mut |learned| {
                side.learn(learned);
            });
        });
    }
    // A live registry reads the library again whenever its structure moves,
    // and scans each directory on its schedule. A fixed one, `serve` handed
    // sidecar paths, does neither.
    if state.works.is_live() {
        registry::spawn_refresher(Arc::clone(&state), registry::REFRESH_EVERY);
        scheduler::spawn(Arc::clone(&state), scheduler::TICK);
    }
    cache_sweep::spawn(&state, cache_max);
    // Named the same way the bind failure is. Both are reported by a process
    // that may be serving several addresses over its life and prints nothing
    // else about them; "Address already in use" without one is a message that
    // has to be guessed at.
    // **`into_make_service_with_connect_info` rather than the router alone**,
    // which is what puts the peer's address in each request's extensions. §6's
    // whole `--trusted-proxy` decision and §7's rate limiter are about *which*
    // address a request is held against, and without this every request arrives
    // with none — so a limiter keyed on the client would key every client
    // together.
    //
    // **Graceful, but not for ever.** `with_graceful_shutdown` waits for every
    // open connection to finish, and a client that never does (a half-sent
    // request head, a download crawling to a slow phone) would hold the
    // process past the container runtime's own stop timeout, and outside a
    // container make Ctrl-C appear to hang. So once the stop signal has
    // resolved, the server gets [`SHUTDOWN_GRACE`] and then `serve` returns
    // whatever is still open.
    let (stopping, stop_began) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(
        listener,
        crate::routes::build_router_serving(state, web.as_deref())
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        stop.await;
        let _ = stopping.send(());
    });
    let grace_over = async move {
        // Resolves with the signal, or with an error if the shutdown future
        // was dropped unsent (it panicked, or the runtime is going away).
        // axum runs that future as its own task, so the server ending by
        // itself does not drop the sender. Whichever way it resolves, the
        // grace starts.
        let _ = stop_began.await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };
    tokio::select! {
        outcome = server => outcome.map_err(|e| format!("{addr}: {e}")),
        () = grace_over => {
            // Not `eprintln!`, which panics when nothing reads stderr, and
            // unwinding from here drops the runtime the normal way, which
            // waits on blocking tasks.
            let _ = writeln!(
                std::io::stderr(),
                "taiyaku serve: closing connections still open"
            );
            Ok(())
        }
    }
}

/// How long [`serve`] lets open connections finish after a stop signal before
/// it returns regardless.
///
/// Chosen to leave room, with the caller's own bound on its runtime, inside
/// the time `docker stop` allows before it kills the container.
pub const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Resolves when the process is asked to stop: SIGTERM, which `docker stop`
/// sends, or SIGINT.
///
/// **Needed because `taiyaku` is PID 1 in the image.** The kernel gives PID 1
/// no default action for a signal it has not installed a handler for, so
/// without this `docker stop` waits out its grace period and then kills the
/// process.
///
/// **Not an `async fn`, so the handlers exist when this returns** rather than
/// when the future is first polled. `serve` calls it before it prints
/// `listening`, which makes that line a promise: a signal sent after it is
/// caught, not one that races the handler's installation.
fn shutdown_signal() -> impl std::future::Future<Output = ()> {
    #[cfg(unix)]
    let (mut terminate, mut interrupt) = {
        use tokio::signal::unix::{SignalKind, signal};
        (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        )
    };
    async move {
        #[cfg(unix)]
        {
            // A handler that cannot be installed must not read as a request
            // to stop, so that arm waits forever and the other decides.
            let terminated = async {
                match terminate.as_mut() {
                    Ok(stream) => stream.recv().await,
                    Err(_) => std::future::pending().await,
                }
            };
            let interrupted = async {
                match interrupt.as_mut() {
                    Ok(stream) => stream.recv().await,
                    Err(_) => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = terminated => {},
                _ = interrupted => {},
            }
        }
        #[cfg(not(unix))]
        {
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
        // Not `eprintln!`: it panics when nothing reads stderr, and that
        // panic would drop the sender unsent.
        let _ = writeln!(std::io::stderr(), "taiyaku serve: stopping");
    }
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
/// Separate from `serve` for the reason `resolve_all_works` is: a test drives
/// the loop without a socket, and the loop is where the behaviour worth
/// pinning lives.
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
/// **The breaker spans works, not just one work's members.** `resolve_work`
/// already stops asking after the first transport-level failure within a work;
/// without hoisting it here too, an unreachable instance would still pay one
/// 30-second POST timeout *per work*, and twenty works is ten minutes. Once
/// any work reports `aborted_by_transport`, every later work is skipped
/// without being asked at all, and that is said once rather than once per
/// skipped work.
///
/// Separate from `serve` so this loop can be driven by a `FakeTransport`
/// without a socket.
fn resolve_all_works<W: std::borrow::Borrow<Work>>(
    transport: &dyn stump::http::Transport,
    works: &[W],
    base: &str,
    learned: &mut dyn FnMut(stump::cache::Resolutions),
) {
    for (index, work) in works.iter().enumerate() {
        let work: &Work = std::borrow::Borrow::borrow(work);
        // O8: `report` prints each member as it is asked about, so a work with
        // many unresolved members is not silent for its whole duration and a
        // slow resolution can be told from a hung one. Members answered from
        // the cache print nothing, so a warm start has no per-member lines.
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

/// How every route addressed by work refuses one this server does not serve.
fn no_work(id: &str) -> String {
    format!("no work `{id}`")
}

/// What to say when a run gives up partway, or `None` when it gave up on the
/// last work and there is nothing left to skip.
///
/// The subtraction and the `> 0` test that guards it decide only the wording of
/// a line on stderr, and the abort test asserts which requests were made rather
/// than what was printed. Extracted so the arithmetic is a value something can
/// check — including the case that matters most, aborting on the final work,
/// where every mutation of the subtraction produces a sentence promising to
/// skip works that do not exist.
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

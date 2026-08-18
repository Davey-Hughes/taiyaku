//! What a work is, and how a URL names one.
//!
//! There is no library scan here. `taiyaku serve` is handed sidecar paths the
//! same way `align` is handed members, because deciding which files constitute
//! a library is Stump's job and doing it badly here would be worse than not
//! doing it.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use taiyaku_core::Sidecar;

/// One work the server can serve, and where its sidecar came from.
///
/// Not to be confused with `taiyaku_core::Work`, which is the `work` block
/// inside a sidecar — a title and a reading direction. This one is a loaded
/// document plus the identity a URL names it by, and reaches that one as
/// `self.sidecar().work`.
///
/// The document is held behind an `RwLock<Arc<_>>` rather than owned outright
/// because `PATCH /api/works/{work}/alignments` replaces it while the server
/// is running: readers clone an `Arc` and are never blocked by each other, and
/// a writer swaps a whole new document in under one exclusive lock. The same
/// shape `StumpSide` already uses for its resolution map, and for the same
/// reason.
///
/// **Deliberately not `Clone`.** A cloned `Work` would carry an independent
/// lock, so a correction applied to one copy would be invisible to the other —
/// two documents claiming one work, which is the failure `load_works` already
/// refuses at startup.
#[derive(Debug)]
pub struct Work {
    pub id: String,
    pub path: PathBuf,
    sidecar: std::sync::RwLock<std::sync::Arc<Sidecar>>,
    /// Serialises one work's notes read-modify-write. See `notes_guard`.
    notes: std::sync::Mutex<()>,
}

impl Work {
    #[must_use]
    pub fn new(id: String, path: PathBuf, sidecar: Sidecar) -> Self {
        Self {
            id,
            path,
            sidecar: std::sync::RwLock::new(std::sync::Arc::new(sidecar)),
            notes: std::sync::Mutex::new(()),
        }
    }

    /// Held across a notes read-compare-write, so `If-Match` means something.
    ///
    /// The sidecar gets this from `update` below, which can hold its own
    /// `RwLock` across the change because the document is cached in memory.
    /// Notes are not cached: they are read once per page load and written when
    /// a reader types, so keeping a second document in memory for the life of
    /// the process would buy nothing and add a second thing that can go stale.
    /// What is still needed is the *exclusion* — two requests that both read
    /// the same document, both find their validator current, and both write
    /// would lose one reader's note with nothing anywhere saying so, which is
    /// the whole failure the header exists to prevent.
    ///
    /// A poisoned lock is taken rather than propagated, the same call
    /// `sidecar()` makes and for the same reason: the file behind it is still
    /// whatever the last successful write left, and refusing every later
    /// request because one handler panicked is the worse answer.
    pub fn notes_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.notes.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The document as it stands. An `Arc` clone, not a copy of the document.
    ///
    /// A poisoned lock is a panic in a handler that already ran, and the
    /// document behind it is still the last one successfully written, so this
    /// takes the value rather than propagating the poison — a work that
    /// refused to serve every subsequent request because one unrelated handler
    /// panicked would be the worse answer.
    pub fn sidecar(&self) -> std::sync::Arc<Sidecar> {
        match self.sidecar.read() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Read, change and replace under one exclusive lock.
    ///
    /// The whole read-modify-write happens inside the guard, which is what
    /// makes `If-Match` mean anything: two devices correcting one work would
    /// otherwise both compare against the same validator, both pass, and the
    /// second would replace the first's correction with a document that never
    /// contained it.
    ///
    /// The closure returns the replacement document *and* whatever the caller
    /// wants back, or a `Response` to refuse with. A refusal leaves the held
    /// document untouched.
    ///
    /// `NotAPageIndex` and `NoSuchWork` (`routes.rs`) wrap `Response` for the
    /// same `clippy::result_large_err` reason this triggers, but each of those
    /// carries one fixed message a single route mints itself, so the wrapper
    /// names something. This `Response` is minted by whichever caller passes
    /// the closure — a 412 for a stale `If-Match`, a 400 for a malformed body,
    /// and so on — so there is no one thing a wrapper here would be *called*.
    ///
    /// **`Box<Response>` would silence the lint**, and an earlier version of
    /// this comment claimed otherwise; boxing moves the payload to the heap
    /// and leaves a pointer, which is exactly the size clippy measures. It is
    /// not taken because it buys nothing here: correcting a misidentified page
    /// is a reader clicking a filmstrip tile, not a hot loop, so the 128 bytes
    /// this allows are not worth an indirection at every call site and an
    /// unwrap at every match. The lint is right about the size and wrong about
    /// whether it matters, which is what an `allow` is for.
    ///
    /// # Errors
    ///
    /// Returns whatever `f` returns as `Err` — a `Response` the caller minted
    /// to refuse the change, such as a 412 for a stale `If-Match` or a 400
    /// for a malformed body — unchanged, and without replacing the held
    /// document. `update` introduces no error of its own; every `Err` here
    /// is one `f` chose to return.
    #[allow(clippy::result_large_err)]
    pub fn update<T>(
        &self,
        f: impl FnOnce(
            &std::sync::Arc<Sidecar>,
        ) -> Result<(std::sync::Arc<Sidecar>, T), axum::response::Response>,
    ) -> Result<T, axum::response::Response> {
        let mut held = match self.sidecar.write() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (next, out) = f(&held)?;
        *held = next;
        Ok(out)
    }
}

/// A URL-safe id for a work, from its sidecar's filename.
///
/// The sidecar has a title but no id, and titles are neither unique across a
/// library nor stable across an edit. The filename is both, and a person
/// looking at a URL can tell which file it came from.
#[must_use]
pub fn work_id(sidecar_path: &Path) -> String {
    let stem = sidecar_path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = stem.strip_suffix(".json").unwrap_or(&stem);
    let slug: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "work".to_string()
    } else {
        trimmed
    }
}

/// Read every sidecar the server was pointed at.
///
/// Two works resolving to the same id is an error rather than a silent
/// shadowing: one of them would be unreachable and nothing would say which.
///
/// # Errors
///
/// Returns an error if any `path` cannot be read — a missing file or a
/// permission refusal, with the message naming the path — if its contents do
/// not parse as a sidecar (`Sidecar::from_json`), or if two paths' filenames
/// slugify to the same work id, in which case the message names both paths
/// and the id they share, since nothing else would tell the person running
/// the server which two files collided.
pub fn load_works(paths: &[PathBuf]) -> Result<Vec<Work>, String> {
    let mut works: Vec<Work> = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let sidecar = Sidecar::from_json(&text)?;
        let id = work_id(path);
        if let Some(clash) = works.iter().find(|w| w.id == id) {
            return Err(format!(
                "{} and {} both name the work `{id}`; rename one",
                clash.path.display(),
                path.display()
            ));
        }
        works.push(Work::new(id, path.clone(), sidecar));
    }
    Ok(works)
}

/// One work as `GET /api/works` (`routes::list_works`) answers it: enough to
/// identify a work, group it under a series and show its cover, without
/// shipping the sidecar itself.
///
/// **`series` and `volume` come from `state.library`'s rows, not from the
/// sidecar** — `taiyaku_core::Sidecar` has no field for either, because a
/// series grouping and a volume number are spec B's scanner's job, not the
/// aligner's.
///
/// **They stopped being `None` when spec B landed.** This paragraph used to
/// say both were empty "honestly rather than by omission: nothing writes
/// `work.series_id` or `work.volume` yet", which was true while
/// `import::sidecar` was the only writer. `derive.rs` writes both now, so a
/// scanned library answers this route with a real series name and volume —
/// measured end to end on 2026-08-18: scanning one corpus edition, exporting
/// it and serving the result gives
/// `{"series":"Go-Toubun no Hanayome","volume":1.0}` here. **It is the first
/// thing the scanner produces that a reader can see.**
///
/// A work with no library row, and one imported from a document rather than
/// derived, both still answer `None` — the aligner has nowhere to record
/// either, which is why the columns exist. So does the `load_works` path, which
/// `taiyaku serve <sidecar>...` no longer takes in production since Task 15,
/// but which this crate's own fixtures and `corpus.rs` still call directly —
/// answers. `title` and `editions` are not looked up a second time here: they
/// come from `work.sidecar()`, which for a `Work` built by `taiyaku-cli`'s
/// `serve_work` is itself projected from rows (`taiyaku_library::project::sidecar`)
/// rather than parsed from a file, so this is still "from rows" for the path
/// Task 15 made the production one.
#[derive(Debug, serde::Serialize)]
pub(crate) struct WorkSummary {
    pub(crate) slug: String,
    pub(crate) title: String,
    pub(crate) series: Option<String>,
    pub(crate) volume: Option<f64>,
    pub(crate) editions: Vec<String>,
    pub(crate) cover: String,
}

/// Build one work's listing entry.
///
/// A bare `SELECT` against `state.library`, never a write — unlike
/// `row_id_for_write`, this is a read and `find_row`'s rule applies: a work
/// this library has never been told to remember stays unremembered rather
/// than being minted a row just because someone listed it.
///
/// `cover` is a thumbnail URL, not bytes: page 0 of the hub edition, through
/// the existing `/api/works/{work}/{edition}/pages/{index}/thumb` route
/// (`routes::get_thumb`) — the same page `opds::cover` already treats as "the"
/// cover of an edition, so this adds no new route and generates no image at
/// list time.
///
/// # Errors
///
/// Returns an error if the `SELECT` against `conn` fails.
pub(crate) fn summary(conn: &rusqlite::Connection, work: &Work) -> Result<WorkSummary, String> {
    let sidecar = work.sidecar();
    let (volume, series): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT w.volume, s.sort_title FROM work w \
             LEFT JOIN series s ON s.id = w.series_id \
             WHERE w.slug = ?1",
            [&work.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("reading library metadata for `{}`: {e}", work.id))?
        .unwrap_or((None, None));

    let cover = format!(
        "/api/works/{}/{}/pages/0/thumb",
        crate::opds::percent_encode(&work.id),
        crate::opds::percent_encode(&sidecar.hub),
    );

    Ok(WorkSummary {
        slug: work.id.clone(),
        title: sidecar.work.title.clone(),
        series,
        volume,
        editions: sidecar.editions.keys().cloned().collect(),
        cover,
    })
}

/// The single seeded user (`taiyaku-library`'s migration 1: `INSERT INTO
/// user (id, name, role) VALUES (1, 'local', 'admin')`) that every note and
/// reading position belongs to until spec A2 gives this server real
/// accounts. `taiyaku-library`'s own `export.rs` carries the identical
/// constant with the identical reasoning; that crate cannot depend on this
/// one, so the constant cannot be shared, only agreed on in two places.
/// Revisit this constant, not its call sites, once a library can hold more
/// than one real account.
pub(crate) const LOCAL_USER: i64 = 1;

/// The library's own numeric id for a work this server already knows by its
/// slug (`work.id`), or `None` where nothing has ever been written against
/// it.
///
/// **A read never creates a row.** `progress::load`, `notes::load` and
/// `truth::load` all treat "no row" as "nothing recorded" and answer
/// accordingly, so a lookup that inserted one as a side effect of a `GET`
/// would turn every first read into a write nobody asked for.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub(crate) fn find_row(conn: &rusqlite::Connection, work: &Work) -> Result<Option<i64>, String> {
    find_row_by_slug(conn, &work.id)
}

/// [`find_row`] by slug alone, for the caller that cannot borrow the `Work`'s
/// document — see [`row_id_for_document`].
fn find_row_by_slug(conn: &rusqlite::Connection, slug: &str) -> Result<Option<i64>, String> {
    conn.query_row("SELECT id FROM work WHERE slug = ?1", [slug], |row| {
        row.get(0)
    })
    .optional()
    .map_err(|e| format!("looking up work `{slug}` in the library: {e}"))
}

/// The library's own numeric id for a work, minted on first write if this is
/// the first state anything has recorded against it.
///
/// **Bridges two identities this task did not get to unify.**
/// `ServeState.works` holds `Work`s keyed by slug (`work.id`, a `String`);
/// `taiyaku-library` keys `progress`, `notes` and `truth` on the numeric
/// `work.id` row `taiyaku_library::import::sidecar` assigns. A `Work` built
/// by `taiyaku-cli`'s `serve_work` — every `taiyaku serve <sidecar>...`
/// invocation since Task 15 — already has a matching row, its slug having
/// come from one, so this is a lookup and never a mint. A `Work` built by
/// `load_works`, which is now this crate's own fixtures and `corpus.rs`
/// only, has never been imported, so nothing in `library`'s `work` table
/// names it yet. Looked up by slug on every call rather than
/// cached on `Work`, because `Work` has to keep its shape (Task 12's whole
/// claim) — there is nowhere on it to cache a numeric id even for the one
/// call site the answer would save a query on.
///
/// A row minted here carries only what the three state tables' foreign keys
/// need to resolve, plus the columns `work` declares `NOT NULL` —
/// `title`/`reading`/`hub`/`sidecar_path`, copied from the sidecar this
/// server already holds in memory rather than reread. **`content_key` is a
/// placeholder**, `bridge:{slug}` rather than a real content hash: nothing
/// here re-derives one (that is `taiyaku_library::identity::content_key`,
/// which wants the hub edition's member hashes in a shape this function has
/// no reason to recompute), and nothing yet reads this row's `content_key`
/// for any purpose `import::sidecar` itself does not already own. It only
/// has to be unique, which the slug it is derived from already is among the
/// works this server was started with.
///
/// `INSERT OR IGNORE` rather than a bare `INSERT`: `put_progress` writes with
/// no `notes_guard` held, so two racing first writes to one work can both
/// reach here before either has committed. Both compute the identical
/// `content_key` and `slug`, so the loser's insert is a deliberate no-op
/// rather than a `UNIQUE` violation, and the `SELECT` that follows finds
/// whichever of the two committed first.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub(crate) fn row_id_for_write(conn: &rusqlite::Connection, work: &Work) -> Result<i64, String> {
    row_id_for_document(conn, &work.id, &work.sidecar(), &work.path)
}

/// [`row_id_for_write`] for a caller that already holds the document.
///
/// `patch_alignments` runs its whole read-modify-write inside `Work::update`,
/// which holds `Work`'s sidecar `RwLock` for writing — and `std::sync::RwLock`
/// is not reentrant, so `work.sidecar()` there would take the read lock on a
/// lock this thread already holds and hang the request forever. The closure is
/// handed the document instead, which is the same `Arc` `sidecar()` would have
/// returned, so this takes the three fields the insert needs directly.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub(crate) fn row_id_for_document(
    conn: &rusqlite::Connection,
    slug: &str,
    sidecar: &Sidecar,
    path: &Path,
) -> Result<i64, String> {
    if let Some(id) = find_row_by_slug(conn, slug)? {
        return Ok(id);
    }
    conn.execute(
        "INSERT OR IGNORE INTO work (content_key, slug, title, reading, hub, sidecar_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            format!("bridge:{slug}"),
            slug,
            sidecar.work.title,
            direction_text(sidecar.work.reading),
            sidecar.hub,
            path.to_string_lossy(),
        ],
    )
    .map_err(|e| format!("registering work `{slug}` in the library: {e}"))?;
    find_row_by_slug(conn, slug)?.ok_or_else(|| {
        format!("work `{slug}` was just registered in the library but cannot be found")
    })
}

/// `taiyaku-library/src/import.rs`'s private `direction_text`, duplicated
/// rather than exposed across the crate boundary: it is one match arm, and
/// the alternative is making a function `pub` whose only reason to exist is
/// this bridge.
fn direction_text(direction: taiyaku_core::Direction) -> &'static str {
    match direction {
        taiyaku_core::Direction::RightToLeft => "rtl",
        taiyaku_core::Direction::LeftToRight => "ltr",
    }
}

#[cfg(test)]
mod tests;

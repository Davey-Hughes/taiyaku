//! What a work is, and how a URL names one.
//!
//! There is no library scan here. `taiyaku serve` is handed sidecar paths the
//! same way `align` is handed members, because deciding which files constitute
//! a library is Stump's job and doing it badly here would be worse than not
//! doing it.

use std::path::{Path, PathBuf};

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

    /// A file beside this work's sidecar, sharing its stem.
    ///
    /// Three files now live to this rule — `taiyaku.progress.json`,
    /// `taiyaku.stump.json` and `taiyaku.notes.json` — and the stem arithmetic
    /// was written twice before the third arrived, which is where a comment
    /// asking whoever changes one to keep the others in step stops being good
    /// enough. A sidecar with no stem at all falls back to `taiyaku`, the
    /// convention this project documents for the file itself.
    pub fn sibling(&self, suffix: &str) -> PathBuf {
        let stem = self.path.file_stem().map_or_else(
            || "taiyaku".to_string(),
            |s| s.to_string_lossy().to_string(),
        );
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}.{suffix}"))
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

#[cfg(test)]
mod tests;

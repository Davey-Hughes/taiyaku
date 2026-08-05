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
        let stem = self
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "taiyaku".to_string());
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
    match trimmed.is_empty() {
        true => "work".to_string(),
        false => trimmed,
    }
}

/// Read every sidecar the server was pointed at.
///
/// Two works resolving to the same id is an error rather than a silent
/// shadowing: one of them would be unreachable and nothing would say which.
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
mod tests {
    use super::*;

    /// A URL has to name a work, and the sidecar carries no id — only a title,
    /// which is neither unique nor stable. The filename is, so the id comes
    /// from there, slugified so it survives a URL without escaping.
    #[test]
    fn a_work_is_named_by_its_sidecar_filename() {
        assert_eq!(
            work_id(Path::new("/library/Go-Toubun no Hanayome.taiyaku.json")),
            "go-toubun-no-hanayome-taiyaku"
        );
        assert_eq!(work_id(Path::new("taiyaku.json")), "taiyaku");
    }

    /// The id comes from the filename, so two sidecars in two directories with
    /// the same filename — `taiyaku.json` beside each work is the convention
    /// this project documents — are one id. Serving both would make one of
    /// them permanently unreachable at every route, with nothing anywhere
    /// saying which one won or that a choice was made at all.
    ///
    /// O4: the code has refused this since M2 and nothing exercised it, so it
    /// was correct by inspection only. The message has to name **both** paths,
    /// because the whole point is that the person running the server cannot
    /// otherwise tell which two files collided.
    #[test]
    fn two_sidecars_claiming_one_work_id_are_refused_rather_than_shadowed() {
        let first = crate::test_support::work("taiyaku-test-works-clash-first");
        let second = crate::test_support::work("taiyaku-test-works-clash-second");

        let refused = load_works(&[first.path.clone(), second.path.clone()]);

        crate::test_support::cleanup("taiyaku-test-works-clash-first");
        crate::test_support::cleanup("taiyaku-test-works-clash-second");

        let message = refused.expect_err("two files naming one work must not load");
        assert!(
            message.contains(&first.path.display().to_string()),
            "must name the sidecar already holding the id: {message}"
        );
        assert!(
            message.contains(&second.path.display().to_string()),
            "must name the sidecar that collided with it: {message}"
        );
        assert!(
            message.contains("taiyaku"),
            "must name the id they both claim: {message}"
        );
    }

    /// The other half of the same guard, so a refusal cannot be spelled
    /// "refuse everything": two sidecars whose filenames differ load together
    /// and keep their own ids.
    #[test]
    fn two_differently_named_sidecars_load_side_by_side() {
        let first = crate::test_support::work("taiyaku-test-works-distinct-first");
        let second = crate::test_support::work("taiyaku-test-works-distinct-second");
        let renamed = second.path.with_file_name("other.taiyaku.json");
        std::fs::rename(&second.path, &renamed).unwrap();

        let loaded = load_works(&[first.path.clone(), renamed]);

        crate::test_support::cleanup("taiyaku-test-works-distinct-first");
        crate::test_support::cleanup("taiyaku-test-works-distinct-second");

        let works = loaded.expect("distinct filenames are distinct works");
        let ids: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["taiyaku", "other-taiyaku"]);
    }

    /// The property the correction route rests on: what a later reader gets
    /// back is what the last write put in, without a restart.
    // `update`'s own `#[allow(clippy::result_large_err)]` does not reach a
    // closure written at the call site — clippy checks each closure as its
    // own item — so the explicit `Result<(), Response>` below needs the same
    // allow, for the same reason.
    #[allow(clippy::result_large_err)]
    #[test]
    fn a_replaced_sidecar_is_what_the_next_reader_sees() {
        let work = crate::test_support::work("taiyaku-test-works-replace");
        let before = work.sidecar();

        let swapped: Result<(), axum::response::Response> = work.update(|current| {
            let mut next = (**current).clone();
            next.work.title = "Renamed".into();
            Ok((std::sync::Arc::new(next), ()))
        });

        let after = work.sidecar();
        crate::test_support::cleanup("taiyaku-test-works-replace");
        swapped.expect("the update runs");
        assert_eq!(before.work.title, "T");
        assert_eq!(after.work.title, "Renamed");
    }
}

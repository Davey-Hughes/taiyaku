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
/// `self.sidecar.work`.
#[derive(Debug, Clone)]
pub struct Work {
    pub id: String,
    pub path: PathBuf,
    pub sidecar: Sidecar,
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
        works.push(Work {
            id,
            path: path.clone(),
            sidecar,
        });
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
}

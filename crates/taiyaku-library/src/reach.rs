//! Reach: the directories a library shows. Spec
//! `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md` §4.
//!
//! A library shows its own directories and every directory of any library
//! nested inside one of them that its walk would reach. Reach is decided by
//! the outer directory alone, asked exactly as its walk asks
//! ([`scan::Excluder::visits`]), and computed from the `root` rows on every
//! call, so an exclude pattern edited at the outer directory changes reach at
//! once.

use std::collections::BTreeSet;
use std::path::Path;

use crate::scan;

/// Each root in `of`, and every root nested strictly inside one of them whose
/// path, relative to that one, its [`scan::Excluder::visits`] as a directory.
/// Ascending, deduplicated.
pub fn reach(roots: &[scan::Root], of: &[i64]) -> Vec<i64> {
    let mut reached = BTreeSet::new();
    for outer in roots.iter().filter(|root| of.contains(&root.id)) {
        reached.insert(outer.id);
        // Built only once a nested root is found: most directories have none.
        let mut excluder = None;
        for inner in roots {
            let Some(rel) = nested_in(&inner.path, &outer.path) else {
                continue;
            };
            let excluder = excluder
                .get_or_insert_with(|| scan::Excluder::build(outer.exclude_globs.as_deref()));
            if excluder.visits(&rel, true) {
                reached.insert(inner.id);
            }
        }
    }
    reached.into_iter().collect()
}

/// Why `outer` does not show `inner`, a root nested strictly inside it, or
/// `None` where it does, or where `inner` is not inside it: asked as
/// [`reach`] asks.
pub fn kept_out(outer: &scan::Root, inner: &scan::Root) -> Option<scan::KeptOut> {
    let rel = nested_in(&inner.path, &outer.path)?;
    scan::Excluder::build(outer.exclude_globs.as_deref()).keeps_out(&rel, true)
}

/// The directories `libraries` show: each one's own roots, then [`reach`].
/// Ascending, deduplicated.
///
/// **The one definition of a library's directory set.** An account's access,
/// a library's page, its counts and feeds, Move's sight lists and "Scan
/// library" all read it, so a library shows, counts and walks the same
/// directories wherever it is asked.
pub fn of_libraries(roots: &[scan::Root], libraries: &[i64]) -> Vec<i64> {
    let own: Vec<i64> = roots
        .iter()
        .filter(|root| root.libraries.iter().any(|id| libraries.contains(id)))
        .map(|root| root.id)
        .collect();
    reach(roots, &own)
}

/// Each library whose directories ([`of_libraries`]) include one of `dirs`,
/// ascending: the libraries whose pages show a scan of `dirs`, an outer
/// library among them wherever one of `dirs` is nested in its reach.
pub fn libraries_reaching(all: &[scan::Root], dirs: &[i64]) -> Vec<i64> {
    let libraries: BTreeSet<i64> = all
        .iter()
        .flat_map(|root| root.libraries.iter().copied())
        .collect();
    libraries
        .into_iter()
        .filter(|library| {
            of_libraries(all, &[*library])
                .iter()
                .any(|dir| dirs.contains(dir))
        })
        .collect()
}

/// The directories a scan of `library` walks: every one it shows
/// ([`of_libraries`]), each walked as its own root by its own rules (the
/// overlapping-libraries spec's decision 16).
///
/// **By path**, so a directory is walked before the directories nested in it,
/// and a nested directory's walk, which takes over the files under it (spec
/// §4), comes after the walk around it.
pub fn walked_by(roots: &[scan::Root], library: i64) -> Vec<scan::Root> {
    let reached = of_libraries(roots, &[library]);
    let mut walked: Vec<scan::Root> = roots
        .iter()
        .filter(|root| reached.contains(&root.id))
        .cloned()
        .collect();
    walked.sort_by(|a, b| a.path.cmp(&b.path));
    walked
}

/// `inner`'s path relative to `outer`, `/`-separated, when `inner` sits
/// strictly inside `outer`.
///
/// **Component-wise** (`Path::strip_prefix`), never a string prefix: `/mx` is
/// not inside `/m`. Both paths are canonical as stored (`scan::add_root_in`).
/// Names are read as the walk reads them (`to_string_lossy`), so a pattern
/// matches here exactly what it matches there.
pub(crate) fn nested_in(inner: &Path, outer: &Path) -> Option<String> {
    let rel = inner.strip_prefix(outer).ok()?;
    let names: Vec<String> = rel
        .components()
        .map(|name| name.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!names.is_empty()).then(|| names.join("/"))
}

#[cfg(test)]
pub(crate) mod tests;

//! A standing check that the server and the aligner agree about what page N is.
//!
//! Gated on `TAIYAKU_CORPUS` and skipped when the archives are absent, so CI
//! stays green without them. Run locally whenever page reading changes.
//!
//! This is the seam the spec names: the aligner sequences an edition with
//! `read_members_in_order`, the server addresses one page with `member_page`,
//! and if they ever disagree the reader shows the wrong page while every hash
//! and page count in the sidecar still matches.

use std::path::PathBuf;
use std::sync::Arc;

use crate::{LocalLibrary, PageSource, ServeState, load_works};

fn corpus_root() -> Option<PathBuf> {
    std::env::var("TAIYAKU_CORPUS").ok().map(PathBuf::from)
}

/// Every page the server serves must be byte-identical to the page the aligner
/// puts at that index. Sampling would not do: the failure mode is an offset
/// that appears at one member boundary and holds for everything after it.
#[test]
fn every_page_the_server_serves_is_the_page_the_aligner_sequenced() {
    let Some(root) = corpus_root() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let sidecar = root.join("gotoubun-v01/taiyaku.json");
    if !sidecar.exists() {
        eprintln!("{} absent — skipping", sidecar.display());
        return;
    }

    let works = load_works(std::slice::from_ref(&sidecar)).expect("sidecar loads");
    let dir = sidecar.parent().unwrap();
    let state = ServeState {
        works,
        source: Arc::new(LocalLibrary),
        cache_dir: std::env::temp_dir().join("taiyaku-corpus-covers"),
        log: None,
    };
    let work = &state.works[0];
    let sidecar = work.sidecar();

    let mut compared = 0usize;
    for (key, edition) in &sidecar.editions {
        let members: Vec<PathBuf> = edition.members.iter().map(|m| dir.join(&m.path)).collect();
        // In the order the array gives, not sorted — `docs/sidecar.md` rule 1.
        // `read_edition` sorts, which is right for the shell globs `align` is
        // handed and wrong here: a sidecar's paths are relative to itself and
        // may point into unrelated trees, so re-sorting them would have this
        // check report a disagreement with neither side wrong.
        let sequenced = taiyaku_read::read_members_in_order(&members).expect("aligner sequences");

        for (index, page) in sequenced.iter().enumerate() {
            let served = state
                .page(work, key, index as u32)
                .unwrap_or_else(|e| panic!("{key} page {index}: {e}"));
            assert_eq!(
                served.bytes, page.bytes,
                "{key} page {index} ({}) differs between server and aligner",
                page.name
            );
            compared += 1;
        }
    }
    eprintln!(
        "{compared} pages compared across {} editions",
        sidecar.editions.len()
    );
    assert!(compared > 0, "the corpus sidecar has no pages to compare");
}

/// The third of the four seam tests the reader design names.
///
/// `LocalLibrary` and the composite's Stump-backed path (`stump::api` and
/// `stump::resolve`; there is no standalone `StumpLibrary` — see
/// `source.rs`) must return the same bytes for the same member and offset.
/// If they ever disagree, a swap shows a different page depending on where
/// the server happened to get it, and nothing else notices.
///
/// Byte-for-byte rather than by `dhash`: five pages were measured identical on
/// 2026-07-31, so Stump does not transcode. If it ever starts, this is the
/// test that says so.
///
/// Gated on `TAIYAKU_CORPUS` **and** a reachable instance, because it is the
/// only test in the suite that talks to one.
#[test]
fn stump_and_local_serve_the_same_bytes_for_the_same_member() {
    let Some(root) = corpus_root() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    // Named separately rather than as one message, so a half-configured run
    // says which half. Guessing between two variables is a small annoyance
    // every time and a free fix once.
    let (Ok(base), Ok(key)) = (
        std::env::var("TAIYAKU_STUMP_URL"),
        std::env::var("TAIYAKU_STUMP_KEY"),
    ) else {
        let missing: Vec<&str> = ["TAIYAKU_STUMP_URL", "TAIYAKU_STUMP_KEY"]
            .into_iter()
            .filter(|name| std::env::var(name).is_err())
            .collect();
        eprintln!("{} unset — skipping", missing.join(" and "));
        return;
    };
    let sidecar = root.join("gotoubun-v01/taiyaku.json");
    if !sidecar.exists() {
        eprintln!("{} absent — skipping", sidecar.display());
        return;
    }

    let works = load_works(std::slice::from_ref(&sidecar)).expect("sidecar loads");
    let work = &works[0];
    let dir = sidecar.parent().unwrap();
    let transport = crate::stump::http::UreqTransport::new(base.clone(), key);
    let resolved =
        crate::stump::resolve::resolve_work(&transport, work, &base, &mut |member, nth, total| {
            eprintln!("  resolving {member} against stump ({nth} of {total})");
        });
    assert!(
        !resolved.aborted_by_transport,
        "resolution against the instance was abandoned by a transport failure partway through, \
         so this comparison would be incomplete"
    );
    let resolutions = resolved.resolutions;

    let mut compared = 0usize;
    let mut skipped = 0usize;
    let sidecar = work.sidecar();
    for edition in sidecar.editions.values() {
        for member in &edition.members {
            // Three states, and only two of them are acceptable here.
            //
            // `Some(None)` is Stump answering that it has not got this member,
            // which is expected and is the finding that shaped this milestone:
            // Stump indexes the English releases and not the raws taiyaku
            // aligns them against.
            //
            // A **missing key** is not that. `resolve_work` inserts an entry
            // for every member it gets an answer about, and abandons the rest
            // of the run on the first transport failure — so an absent key
            // means this member was never asked about. Counting it as absence
            // would let a network blip shrink the comparison while the test
            // still printed a plausible total and passed, which is exactly the
            // "quietly covers less than it claims" failure this check exists
            // to prevent one level down.
            let id = match resolutions.get(&member.sha256) {
                Some(Some(id)) => id,
                Some(None) => {
                    skipped += 1;
                    continue;
                }
                None => panic!(
                    "{} was never resolved against stump, so this comparison would be \
                     incomplete — resolution was abandoned partway, which means a transport \
                     failure rather than an answer",
                    member.path
                ),
            };
            for within in 0..member.pages {
                let ours = LocalLibrary
                    .page(dir, member, within)
                    .unwrap_or_else(|e| panic!("{} page {within}: {e}", member.path));
                let theirs = crate::stump::api::page_bytes(&transport, id, within)
                    .map_err(|e| e.message)
                    .unwrap_or_else(|e| panic!("{} stump page {within}: {e}", member.path));
                assert_eq!(
                    ours.bytes, theirs.bytes,
                    "{} page {within} differs between disk and stump",
                    member.path
                );
                compared += 1;
            }
        }
    }

    eprintln!("{compared} pages compared against stump; {skipped} members stump does not hold");
    assert!(
        compared > 0,
        "no member of this work resolved against stump, so nothing was compared"
    );
}

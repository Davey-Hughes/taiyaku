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

use crate::{LocalLibrary, PageSource, load_works};

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
    let work = &works[0];
    let dir = sidecar.parent().unwrap();

    let mut compared = 0usize;
    for (key, edition) in &work.sidecar.editions {
        let members: Vec<PathBuf> = edition.members.iter().map(|m| dir.join(&m.path)).collect();
        // In the order the array gives, not sorted — `docs/sidecar.md` rule 1.
        // `read_edition` sorts, which is right for the shell globs `align` is
        // handed and wrong here: a sidecar's paths are relative to itself and
        // may point into unrelated trees, so re-sorting them would have this
        // check report a disagreement with neither side wrong.
        let sequenced = taiyaku_read::read_members_in_order(&members).expect("aligner sequences");

        for (index, page) in sequenced.iter().enumerate() {
            let served = LocalLibrary
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
        work.sidecar.editions.len()
    );
    assert!(compared > 0, "the corpus sidecar has no pages to compare");
}

//! The standing check `notes.md` asks for under "Validate it against all-pairs,
//! and keep checking": that composing through the hub still agrees with a
//! direct solve.
//!
//! Gated on `TAIYAKU_CORPUS` and skipping when it is unset, because the
//! archives are 24 GB and are never committed. CI stays green without them; run
//! it locally whenever the hasher or the solver changes.
//!
//! ```sh
//! TAIYAKU_CORPUS=corpus cargo test -p taiyaku-cli corpus:: -- --nocapture
//! ```
//!
//! This measures the path a reader walks, which `topology` does not: `topology`
//! composes two independently solved alignments over unsplit pages, while a
//! sidecar stores one edge per spoke, inverts it, and addresses slots. The two
//! are the same property measured differently and their figures are not
//! expected to match.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use taiyaku_core::{Half, Sidecar, Slot, counterpart, solve};

use crate::cached_edition;

/// Loss above which the star is not doing its job.
///
/// Measured, not chosen: this path loses **0.91%** of pages on Go-Toubun v01,
/// over 2,427 comparisons, and the ceiling sits at the next tenth of a percent
/// above that. If a change pushes past it, composition has started losing pages
/// a direct solve finds, which is the thing hub-and-spoke is a bet against.
///
/// Lower than `topology`'s 2.12% for the same volume, and the gap is the point
/// of the two measurements being separate: `topology` works on unsplit pages,
/// so a spread one edition joins and another splits is simply lost, while this
/// path addresses slots and recovers it.
const MAX_LOSS: f64 = 0.010;

/// The corpus root, or `None` when this machine does not have it.
fn corpus() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var("TAIYAKU_CORPUS").ok()?);
    root.is_dir().then_some(root)
}

/// The members of one edition directory: its `.cbz` files, or the directory
/// itself when it holds loose page images.
///
/// Order does not matter — `taiyaku_read::read_edition` sorts naturally, which it has
/// to, because `c005.5.cbz` sorts before `c005.cbz` in every shell glob.
fn members(dir: &Path) -> Vec<String> {
    let archives: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.extension()?
                .eq_ignore_ascii_case("cbz")
                .then(|| path.to_string_lossy().to_string())
        })
        .collect();
    match archives.is_empty() {
        true => vec![dir.to_string_lossy().to_string()],
        false => archives,
    }
}

/// Build the five-edition Go-Toubun document through the merge path, then check
/// every spoke-to-spoke lookup against a direct solve.
///
/// Disagreement is the number that decides it — a reader landing on a different
/// page depending on which edition it swapped through was the theoretical worry
/// hub-and-spoke carried, and it must be zero. Loss is expected and safe: the
/// reader sees "no counterpart" rather than the wrong page.
#[test]
fn composing_through_the_hub_agrees_with_a_direct_solve() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let volume = root.join("gotoubun-v01");
    let hub = "raw-mono";
    let spokes = ["raw-color", "en-mono", "en-color", "en-fan"];

    // The cache belongs beside the corpus, not in the working directory, which
    // for `cargo test -p` is the crate directory rather than the repository
    // root. Sharing the one `align` already fills is the difference between
    // milliseconds and seconds per edition — 3.4 s cold against 189 ms warm.
    let cache = root
        .parent()
        .unwrap_or(Path::new("."))
        .join(".taiyaku-cache");

    let out = std::env::temp_dir().join("taiyaku-corpus-star.json");
    let _ = std::fs::remove_file(&out);
    for spoke in spokes {
        let mut args = members(&volume.join(hub));
        args.push("--".into());
        args.extend(members(&volume.join(spoke)));
        args.extend([
            "--split-spreads".into(),
            "--write".into(),
            out.to_string_lossy().to_string(),
            "--cache".into(),
            cache.to_string_lossy().to_string(),
            "--title".into(),
            "Go-Toubun no Hanayome".into(),
            "--name-a".into(),
            hub.into(),
            "--name-b".into(),
            spoke.into(),
        ]);
        crate::align(&args).unwrap_or_else(|e| panic!("aligning {spoke}: {e}"));
    }

    let sidecar = Sidecar::from_json(&std::fs::read_to_string(&out).expect("written"))
        .expect("the document we just wrote parses");
    assert_eq!(
        sidecar.editions.len(),
        5,
        "five editions, one hub, four edges"
    );
    assert_eq!(sidecar.alignments.len(), 4);
    assert_eq!(sidecar.hub, hub, "merging must never move the hub");

    let mut fingerprints = BTreeMap::new();
    for edition in spokes {
        let paths: Vec<PathBuf> = members(&volume.join(edition))
            .iter()
            .map(PathBuf::from)
            .collect();
        let (hashes, table, _) =
            cached_edition(&cache, &paths, sidecar.work.reading).expect("fingerprints");
        fingerprints.insert(edition, (hashes, table));
    }

    let (mut compared, mut loss, mut disagree, mut blank) = (0u32, 0u32, 0u32, 0u32);
    for from in spokes {
        for to in spokes {
            if from == to {
                continue;
            }
            let (from_hashes, from_slots) = &fingerprints[from];
            let (to_hashes, to_slots) = &fingerprints[to];
            let direct = solve(from_hashes, to_hashes);
            let route = taiyaku_core::route(&sidecar, from, to)
                .unwrap_or_else(|| panic!("no route from {from} to {to}"));

            // Both sides are asked the same question: where does the FIRST slot
            // of this page land? A spread's second half is not addressable by
            // page number, so comparing per slot would count a half no caller
            // can ask for.
            for (index, slot) in from_slots.iter().enumerate() {
                if slot.half == Some(Half::Left) {
                    continue;
                }
                // A dhash asks, for each of 256 pixel pairs, whether the left
                // is brighter than the right. On a blank page none is, so the
                // hash is exactly zero — and every blank page in every edition
                // carries that same hash. Where a solve sends such a page is
                // arbitrary, so comparing two arbitrary answers measures
                // nothing about composition. Counted, not silently dropped.
                //
                // This is notes.md's "Blank pages match each other perfectly
                // and lie about it": the score is not merely high, it is
                // perfect. Refusing to anchor on them is open work in the
                // solver; this test only declines to measure them.
                if from_hashes[index].0 == [0u64; 4] {
                    blank += 1;
                    continue;
                }
                compared += 1;
                let expected = counterpart(&direct, index as u32)
                    .and_then(|i| to_slots.get(i as usize))
                    .copied();
                let got: Option<Slot> = route.resolve(slot.page);
                match (expected, got) {
                    (Some(_), None) => loss += 1,
                    (Some(x), Some(y)) if x != y => {
                        disagree += 1;
                        if disagree <= 5 {
                            let bits: u32 =
                                from_hashes[index].0.iter().map(|w| w.count_ones()).sum();
                            eprintln!(
                                "  {from} p{} -> {to}: direct {x:?} composed {y:?} ({bits} bits set)",
                                slot.page
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let loss_rate = loss as f64 / compared as f64;
    eprintln!(
        "{compared} page comparisons | loss {:.2}% | disagreement {:.2}% \
         | {blank} blank pages not compared",
        100.0 * loss_rate,
        100.0 * disagree as f64 / compared as f64
    );
    assert_eq!(
        disagree, 0,
        "composed and direct must not send a page to two different places"
    );
    assert!(
        loss_rate <= MAX_LOSS,
        "loss {loss_rate:.4} exceeds {MAX_LOSS}"
    );
}

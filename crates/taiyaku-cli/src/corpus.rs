//! The standing check `notes.md` asks for under "Validate it against all-pairs,
//! and keep checking": that composing through the hub still agrees with a
//! direct solve.
//!
//! Gated on `TAIYAKU_CORPUS` and skipping when it is unset, because the
//! archives are 24 GB and are never committed. CI stays green without them; run
//! it locally whenever the hasher or the solver changes.
//!
//! ```sh
//! scripts/check-all.sh                    # TAIYAKU_CORPUS=corpus works here
//! ```
//!
//! **Absolute, if you run cargo directly.** Cargo gives each package's test
//! binary that package's own directory as its working directory, so a relative
//! `TAIYAKU_CORPUS=corpus` resolves under `crates/taiyaku-cli/`. `corpus`
//! panics rather than skipping in that case, so the command fails loudly
//! instead of printing `ok`, and the working invocation is:
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli corpus:: -- --nocapture
//! ```
//!
//! `check-all.sh` canonicalises the path itself and is the better habit.
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
/// and the ceiling sits at the next tenth of a percent above that. If a change
/// pushes past it, composition has started losing pages a direct solve finds,
/// which is the thing hub-and-spoke is a bet against.
///
/// Lower than `topology`'s loss for the same volume, and the gap is the point
/// of the two measurements being separate: `topology` works on unsplit pages,
/// so a spread one edition joins and another splits is simply lost, while this
/// path addresses slots and recovers it.
const MAX_LOSS: f64 = 0.010;

/// Why `value` cannot be used as a corpus root, or `None` when it can.
///
/// Split out from `corpus` so the message can be tested. The message is the
/// whole point of the guard, and the alternative — setting the environment
/// variable inside a test — is process-global and racy under a parallel runner.
fn unusable(value: &str, is_dir: bool) -> Option<String> {
    if is_dir {
        return None;
    }
    let cwd = std::env::current_dir().map_or_else(
        |_| "the working directory".to_string(),
        |d| d.display().to_string(),
    );
    Some(format!(
        "TAIYAKU_CORPUS={value} is not a directory from {cwd}.\n\
         cargo runs each package's tests with that package's own directory as \
         the working directory rather than the workspace root, so a relative \
         path resolves under crates/<package>/ and finds nothing. Pass an \
         absolute path, or run scripts/check-all.sh, which canonicalises it."
    ))
}

/// The corpus root, or `None` when this machine does not have it.
///
/// **Unset is an absence and skips; set-but-unusable is a mistake and fails.**
/// O30: a green run that measured nothing is the one outcome a standing check
/// must not have, and it is the same failure `atomic.rs`'s test comment
/// describes one level down.
///
/// The panic is O15's rule applied to an environment variable: a value typed
/// with nothing usable behind it is refused rather than read as absent.
/// `pub(crate)` for `geometry_experiment`, which is gated the same way and must
/// gate it the *same* way rather than carrying a second copy of this rule —
/// unset skips, set-but-unusable panics. Two spellings of one gate is how one
/// of them ends up skipping in silence, which is the failure O30 closed.
pub(crate) fn corpus() -> Option<PathBuf> {
    let value = std::env::var("TAIYAKU_CORPUS").ok()?;
    let root = PathBuf::from(&value);
    if let Some(why) = unusable(&value, root.is_dir()) {
        panic!("{why}");
    }
    Some(root)
}

/// The members of one edition directory: its `.cbz` files, or the directory
/// itself when it holds loose page images.
///
/// Order does not matter — `taiyaku_read::read_edition` sorts naturally, which it has
/// to, because `c005.5.cbz` sorts before `c005.cbz` in every shell glob.
pub(crate) fn members(dir: &Path) -> Vec<String> {
    let archives: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.extension()?
                .eq_ignore_ascii_case("cbz")
                .then(|| path.to_string_lossy().to_string())
        })
        .collect();
    if archives.is_empty() {
        vec![dir.to_string_lossy().to_string()]
    } else {
        archives
    }
}

/// A slot index, as `counterpart` takes it.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot at this test's one call site:
/// `i` comes from `.enumerate()` over `from_slots`, one entry per slot of a
/// real scanned volume under `TAIYAKU_CORPUS`; volumes here run to a few
/// hundred pages, nowhere near `u32::MAX`.
fn slot_index(i: usize) -> u32 {
    u32::try_from(i).expect("slot index exceeds u32::MAX")
}

/// Build the five-edition Go-Toubun document through the merge path, then check
/// every spoke-to-spoke lookup against a direct solve.
///
/// Disagreement is the number that decides it — a reader landing on a different
/// page depending on which edition it swapped through was the theoretical worry
/// hub-and-spoke carried, and it must be zero. Loss is expected and safe: the
/// reader sees "no counterpart" rather than the wrong page.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one measurement in three necessary parts — build the five-edition document \
              through align's own hub-and-spoke merge path, fingerprint every spoke, then \
              compare every spoke-to-spoke composed lookup against a direct solve — and the \
              claim this test makes (`disagree == 0`) is only true of all three run together. \
              Splitting the comparison loop out would need `sidecar`, `fingerprints`, `spokes` \
              and the four running counters passed across the boundary for no reduction in \
              what a reader has to hold at once; the doc comments on the blank-page skip and \
              the half-slot skip inside the loop explain choices specific to this comparison, \
              not general-purpose helpers worth naming on their own."
)]
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
    // milliseconds and seconds per edition.
    let cache = root
        .parent()
        .unwrap_or(Path::new("."))
        .join(".taiyaku-cache");

    let scratch = taiyaku_read::test_support::TempDir::new("corpus-star");
    let out = scratch.join("star.json");
    for spoke in spokes {
        let mut args = members(&volume.join(hub));
        args.push("--".into());
        args.extend(members(&volume.join(spoke)));
        args.extend([
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
                let expected = counterpart(&direct, slot_index(index))
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

    let loss_rate = f64::from(loss) / f64::from(compared);
    eprintln!(
        "{compared} page comparisons | loss {:.2}% | disagreement {:.2}% \
         | {blank} blank pages not compared",
        100.0 * loss_rate,
        100.0 * f64::from(disagree) / f64::from(compared)
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

/// The cache the duplicate checks share with `align`, beside the corpus rather
/// than in the working directory — which for `cargo test -p` is the crate
/// directory. Same reasoning as the star check above.
fn duplicate_cache(root: &Path) -> PathBuf {
    root.parent()
        .unwrap_or(Path::new("."))
        .join(".taiyaku-cache")
}

fn copies(root: &Path, under: &str, names: &[&str]) -> Vec<Vec<PathBuf>> {
    names
        .iter()
        .map(|name| vec![root.join("incoming").join(under).join(name)])
        .collect()
}

/// One Piece volume 77: the case that argues for all pairs rather than every
/// copy against the first.
///
/// c0 and c1 hold 234 images each and c2 holds 214 — and it is c1 and c2 that
/// are one edition, the smaller being all-spread and expanding to the same 234
/// slots. c0 is exactly the copy most likely to be the odd one out because the
/// sort that produced it was by page count.
///
/// This check goes red if step 2 is ever rewritten as against-copy-0.
#[test]
fn duplicates_finds_the_identity_hidden_behind_the_first_copy() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "op-duplicates",
        &["v077-mono-c0", "v077-mono-c1", "v077-mono-c2"],
    );

    let resolved = crate::duplicates::resolve(
        &set,
        None,
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("v077 resolves");

    eprintln!("v077: {}", crate::duplicates::set_line(&resolved));
    eprintln!(
        "v077 slots: {:?} | clusters {:?}",
        resolved.copies.iter().map(|c| c.slots).collect::<Vec<_>>(),
        resolved.clusters
    );
    let merged: Vec<&Vec<usize>> = resolved.clusters.iter().filter(|c| c.len() > 1).collect();
    assert_eq!(
        merged.len(),
        1,
        "exactly one identity cluster: {:?}",
        resolved.clusters
    );
    // `cluster()`'s own doc comment guarantees members come back ascending, so
    // the pair can be asserted directly rather than through length-and-contains,
    // which would pass just as well for the wrong pair {0, 2} as for the right
    // one {1, 2}.
    assert_eq!(
        merged[0],
        &vec![1, 2],
        "copy 1 and copy 2 are the identity pair, not copy 0: {:?}",
        resolved.clusters
    );
    assert!(resolved.contradictions.is_empty());
    assert!(resolved.flagged.is_empty(), "all three are volume 77");

    let identity = resolved
        .pairs
        .iter()
        .find(|p| p.relation == crate::duplicates::Relation::Identity)
        .expect("an identity pair");
    assert_eq!(
        identity.runs, 1,
        "full coverage on both sides is one run, or run compression is broken"
    );
}

/// One Piece volume 92: 200 images against 101, the smaller being an
/// all-spread edition.
///
/// Every one of c1's 101 images detects as a spread, so slot expansion doubles
/// it to 202 — two more slots than c0's 200. The solve runs
/// `s0=0-s199=199 -> s1=0L-s200=100R`, pairing all 200 of c0's slots but
/// leaving c1's first and last half-slots unreached on the far side. That is
/// coverage on one side only, which is `Within` by definition, not `Identity` —
/// c0 is wholly contained in c1's larger packing, and the two stay two clusters
/// with c1 ranked first on its 202 slots.
///
/// `align`'s "N unmatched slots" counts the A side only. Goes red if slot
/// expansion or the containment relation stops being load-bearing here.
#[test]
fn duplicates_sees_through_a_different_pagination() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(&root, "op-duplicates", &["v092-mono-c0", "v092-mono-c1"]);

    let resolved = crate::duplicates::resolve(
        &set,
        None,
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("v092 resolves");

    eprintln!("v092: {}", crate::duplicates::set_line(&resolved));
    eprintln!(
        "v092 slots: {:?} | clusters {:?}",
        resolved.copies.iter().map(|c| c.slots).collect::<Vec<_>>(),
        resolved.clusters
    );
    assert_eq!(resolved.copies[0].slots, 200);
    assert_eq!(resolved.copies[1].slots, 202);
    assert_eq!(
        resolved.clusters,
        vec![vec![0], vec![1]],
        "containment, not identity: two clusters"
    );
    assert_eq!(
        resolved.pairs[0].relation,
        crate::duplicates::Relation::Within
    );
    assert_eq!(resolved.pairs[0].paired, 200, "all of c0's slots pair");
    assert_eq!(
        resolved.ranked[0].cluster,
        vec![1],
        "c1 carries more slots and ranks first"
    );
}

/// The Bocchi file named as main-series volume 1 that is the Gaiden spinoff.
///
/// What is asserted is the flagging: it refuses against both real scans, is kept
/// out of the ranking, and the survivors rank by slots. Also asserted: the two
/// real scans do not cluster as one. `hamaji` (125 slots) *contains* `krc`
/// (124), 124 paired — containment, one slot apart, not identity — and
/// containment does not merge, so the two stay separate clusters. Three
/// clusters result: `hamaji`, `krc`, and the Gaiden file refusing against both.
#[test]
fn duplicates_flags_a_file_that_is_not_the_volume_it_claims() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "bocchi",
        &[
            "raw-v01-src-hamaji",
            "raw-v01-src-krc",
            "raw-v01-src-mangazip",
        ],
    );

    let resolved = crate::duplicates::resolve(
        &set,
        None,
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("bocchi resolves");

    eprintln!("bocchi: {}", crate::duplicates::set_line(&resolved));
    eprintln!(
        "bocchi slots: {:?} | clusters {:?}",
        resolved.copies.iter().map(|c| c.slots).collect::<Vec<_>>(),
        resolved.clusters
    );
    assert_eq!(
        resolved.clusters,
        vec![vec![0], vec![1], vec![2]],
        "three clusters: hamaji contains krc one slot apart, and the Gaiden file refuses both"
    );
    assert_eq!(resolved.flagged, vec![2], "the Gaiden file, and only it");
    assert!(
        resolved.ranked.iter().all(|r| !r.cluster.contains(&2)),
        "a flagged copy must not be ranked"
    );
    let places: Vec<usize> = resolved.ranked.iter().map(|r| r.slots).collect();
    assert!(
        places.windows(2).all(|w| w[0] >= w[1]),
        "ranked by slots descending: {places:?}"
    );
}

/// The `--against` flagging rule's corpus witness: the same three Bocchi
/// copies as the check above, judged twice over — once as a bare set, once
/// against the one edition that can actually tell an impostor from a set of
/// impostors.
///
/// Without a reference, `mangazip` is the copy flagged: it refuses both real
/// scans and they refuse it right back, so the set alone reads it as not this
/// volume. But `mangazip` is not an impostor of nothing — it is the *Gaiden*
/// spinoff filed under the main series' name, and the corpus holds the real
/// edition it belongs to: `en-gaiden-v01.cbz`. `docs/notes.md` records that
/// pairing and records the main-series raws refusing that same edition. Handing
/// it to `--against` inverts the flagging: `mangazip` pairs with the reference
/// and is ranked, while `hamaji` and `krc` — the two real scans of the volume
/// the set is actually named for — pair nothing with it and are the ones
/// flagged instead. A reference edition is the only thing in this protocol that
/// can distinguish an impostor from a set of impostors, and this is the one
/// case in the corpus where handing it one does not merely confirm the set's
/// own reading but reverses it outright: the copy the set calls the odd one out
/// is the only one that really is this volume, once judged against the edition
/// it belongs to.
///
/// `resolve` runs twice over the same three directories, once with no
/// reference and once against the Gaiden edition, so the inversion is
/// asserted rather than described and the check does not depend on reading
/// the check above first.
#[test]
fn duplicates_against_the_gaiden_edition_inverts_which_copy_is_flagged() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "bocchi",
        &[
            "raw-v01-src-hamaji",
            "raw-v01-src-krc",
            "raw-v01-src-mangazip",
        ],
    );
    let cache = duplicate_cache(&root);

    let without_reference =
        crate::duplicates::resolve(&set, None, taiyaku_core::Direction::RightToLeft, &cache)
            .expect("bocchi resolves with no reference");
    eprintln!(
        "bocchi, no reference:          {}",
        crate::duplicates::set_line(&without_reference)
    );

    let reference = vec![root.join("incoming/bocchi/en-gaiden-v01.cbz")];
    let against_gaiden = crate::duplicates::resolve(
        &set,
        Some(&reference),
        taiyaku_core::Direction::RightToLeft,
        &cache,
    )
    .expect("bocchi resolves against the Gaiden edition");
    eprintln!(
        "bocchi, against en-gaiden-v01.cbz: {}",
        crate::duplicates::set_line(&against_gaiden)
    );

    assert_eq!(
        without_reference.flagged,
        vec![2],
        "without a reference, mangazip (the Gaiden file) is the one flagged"
    );
    assert_eq!(
        against_gaiden.flagged,
        vec![0, 1],
        "against its real edition, hamaji and krc pair nothing with it and are flagged instead"
    );
    assert_eq!(
        against_gaiden.ranked.len(),
        1,
        "only mangazip pairs with the reference: {:?}",
        against_gaiden.ranked
    );
    assert_eq!(
        against_gaiden.ranked[0].cluster,
        vec![2],
        "the copy the set alone flagged is the one ranked, once judged against its own edition"
    );
}

/// Base selection against a reference edition: four Chainsaw Man raws against
/// the English release. `src-b` holds 199 images to the others' 198 and wins on
/// content.
///
/// Also asserted: the cluster shape. `src-a` and `src-c` are true identity at
/// 198 slots each and merge into one cluster; `src-b` stands alone on its 199th
/// slot; the colour edition stands alone too, matching neither.
#[test]
fn duplicates_ranks_scans_against_a_reference_edition() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let set = copies(
        &root,
        "chainsawman",
        &[
            "raw-v01-src-a",
            "raw-v01-src-b",
            "raw-v01-src-c",
            "raw-v01-colour",
        ],
    );
    let against = vec![root.join("incoming/chainsawman/en-v01.cbz")];

    let resolved = crate::duplicates::resolve(
        &set,
        Some(&against),
        taiyaku_core::Direction::RightToLeft,
        &duplicate_cache(&root),
    )
    .expect("chainsawman resolves");

    eprintln!("chainsawman: {}", crate::duplicates::set_line(&resolved));
    eprintln!("chainsawman clusters: {:?}", resolved.clusters);
    assert_eq!(
        resolved.clusters,
        vec![vec![0, 2], vec![1], vec![3]],
        "src-a and src-c merge as identity; src-b and the colour edition stand alone"
    );
    let winner = &resolved.ranked[0];
    assert!(
        winner.cluster.contains(&1),
        "the 199-image scan wins on content: {:?}",
        resolved.ranked
    );
    assert!(
        resolved.ranked.iter().all(|r| r.against.is_some()),
        "a reference edition was given, so every row carries its figures: {:?}",
        resolved.ranked
    );
}

/// The inventory over the duplicate corpus: every set found, and the parse
/// surviving real names.
#[test]
fn the_inventory_finds_every_duplicate_set() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let under = root.join("incoming/op-duplicates");

    let walked = crate::inventory::walk(&under, 60).expect("the corpus walks");

    eprintln!(
        "op-duplicates: {} candidates, excluded {:?}",
        walked.candidates.len(),
        walked.excluded
    );
    assert_eq!(walked.candidates.len(), 34, "34 copies");

    let mut volumes: Vec<u32> = walked
        .candidates
        .iter()
        .map(|c| {
            let name = c
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            taiyaku_read::volume_number(&name)
                .unwrap_or_else(|| panic!("no volume number parsed from `{name}`"))
        })
        .collect();
    volumes.sort_unstable();
    volumes.dedup();
    assert_eq!(volumes.len(), 16, "16 volumes: {volumes:?}");
    assert!(volumes.contains(&95), "v095-colour-c0 parses to 95");
}

/// Load one work's sidecar, or panic naming the path — every corpus check
/// below reads a `taiyaku.json` before doing anything else, and a missing or
/// unparsable one is a corpus setup problem, not a thing the assertions below
/// should have to explain.
fn read_sidecar(volume: &Path) -> Sidecar {
    let path = volume.join("taiyaku.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Sidecar::from_json(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Go-Toubun v01's **hub** reports the pages its English edition never reaches.
///
/// The other two credits checks in this file both ask `("en-fan", "raw-mono")`
/// — the `to` side of a stored edge — because that is the direction a reader
/// asks about a spoke. This one asks the `from` side (O38). A defect whose
/// failure mode is "prints nothing" needs a check that expects something.
///
/// The pages are asserted, not just the count: two-of-two is reachable by
/// reporting the wrong two.
#[test]
fn gotoubun_hub_reports_the_pages_its_english_edition_never_reaches() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let doc = read_sidecar(&root.join("gotoubun-v01"));

    let runs = taiyaku_core::unpaired_runs(&doc, "raw-mono", "en-mono").expect("aligned");
    let pages: Vec<u32> = runs.iter().map(|r| r.pages.start).collect();

    println!(
        "gotoubun raw-mono against en-mono: {} unpaired runs at {:?}",
        runs.len(),
        pages
    );

    assert_eq!(
        pages,
        vec![1, 6],
        "the hub's own pages 1 and 6 have no counterpart in en-mono; \
         an empty answer here is O38 returning"
    );
    for run in &runs {
        assert_eq!(run.pages.end - run.pages.start, 1, "each gap is one page");
        assert_eq!(run.member, 0, "raw-mono is a single member");
    }
}

/// Go-Toubun v01's fan edition classifies exactly its four credit pages.
///
/// Exercises the actual classification rule — `credits::credit_pages`, the
/// pure function `command` calls once it has `unpaired_runs` in hand — rather
/// than a copy of it: `credit_pages` reads only run geometry, no page hashes,
/// so calling it directly here is the same code path `command` runs and needs
/// no fingerprinting to reach.
///
/// Asserts the *set*, not the count: four-of-six is reachable by classifying
/// the wrong four, taking `c005.5`'s front matter and leaving `c005`'s staff
/// credits behind. See `docs/notes.md`,
/// "A second group's habits, and what the bound is worth".
///
/// The six unpaired runs are asserted separately from the four classified
/// pages: a regression in boundary splitting (`unpaired_runs`) and a
/// regression in the classification rule (`credit_pages`, via
/// `MAX_CREDIT_RUN`) are two different ways for credits detection to break,
/// and collapsing both into one assertion would hide which one did.
#[test]
fn gotoubun_fan_edition_classifies_four_credit_pages() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let doc = read_sidecar(&root.join("gotoubun-v01"));

    let runs = taiyaku_core::unpaired_runs(&doc, "en-fan", "raw-mono").expect("aligned");
    let pages = crate::credits::credit_pages(&runs);

    println!(
        "gotoubun en-fan: {} unpaired runs, classified {:?}",
        runs.len(),
        pages
    );

    assert_eq!(
        runs.len(),
        6,
        "six unpaired runs, or boundary splitting has regressed"
    );
    assert_eq!(
        pages,
        vec![95, 168, 198, 225],
        "the classified set must be exactly these four pages, not merely four of six"
    );
}

/// Kaguya-sama v01's fan edition classifies what the survey measured: one
/// page, and it is a known false positive.
///
/// Same code path as the Go-Toubun check above: `unpaired_runs` then
/// `credits::credit_pages`, the pure function `command` calls, called
/// directly rather than reimplemented.
///
/// **212 is a FALSE POSITIVE.** It is the publisher's full-colour
/// promotional announcement ("VOLUME 1 WILL BE RELEASED ON 3/18", the series
/// moving to Young Jump as a weekly) — not scanlation matter, and not what
/// the rule means to find. It is pinned here anyway, because this check's
/// job is to notice when the tool's output *changes*, not to bless what it
/// outputs today. **If a future improvement to the classification rule makes
/// this test fail because 212 stops being classified, that is the correct
/// outcome: update this expectation, do not restore the behaviour that keeps
/// it passing.**
///
/// **This check also pins the solver, not only the classifier.**
/// `corpus/kaguya-v01/taiyaku.json` comes from an alignment the solver itself
/// rated `FRAGMENTED`. `unpaired_runs` reads that alignment directly, so a
/// change to the solver, unrelated to credit pages at all, can move this
/// check's numbers for reasons that have nothing to do with the rule under
/// test. See `docs/notes.md`,
/// "A second group's habits, and what the bound is worth".
#[test]
fn kaguya_fan_edition_classifies_what_the_survey_measured() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let doc = read_sidecar(&root.join("kaguya-v01"));

    let runs = taiyaku_core::unpaired_runs(&doc, "en-fan", "raw-mono").expect("aligned");
    let pages = crate::credits::credit_pages(&runs);

    println!(
        "kaguya en-fan: {} unpaired runs, classified {:?} (page 212 is a KNOWN FALSE POSITIVE \
         — a publisher promo, not scanlation matter)",
        runs.len(),
        pages
    );

    assert_eq!(
        runs.len(),
        9,
        "nine unpaired runs, or the FRAGMENTED alignment this check depends on has changed"
    );
    assert_eq!(
        pages,
        vec![212],
        "212 is a known false positive; if this goes red because 212 is no longer classified, \
         that is correct — update this expectation, not the rule"
    );
}

/// O30's guard, tested where the environment cannot be.
///
/// These do not need the corpus and are not gated on it — the whole point is
/// that they run in CI, where `TAIYAKU_CORPUS` is unset and every check in this
/// file skips.
#[cfg(test)]
mod tests;

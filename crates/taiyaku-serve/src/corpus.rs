//! A standing check that the server and the aligner agree about what page N is.
//!
//! Gated on `TAIYAKU_CORPUS` and skipped when the archives are absent, so CI
//! stays green without them. Run locally whenever page reading changes.
//!
//! ```sh
//! scripts/check-all.sh                    # TAIYAKU_CORPUS=corpus works here
//! ```
//!
//! **Absolute, if you run cargo directly.** cargo gives each package's test
//! binary that package's own directory as its working directory, not the
//! workspace root, so a relative `corpus` resolves under `crates/taiyaku-serve/`
//! and finds nothing. Since O30 that fails loudly rather than skipping, but the
//! working invocation is:
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-serve corpus:: -- --nocapture
//! ```
//!
//! `check-all.sh` canonicalises the path itself and is the better habit.
//!
//! This is the seam the spec names: the aligner sequences an edition with
//! `read_members_in_order`, the server addresses one page with `member_page`,
//! and if they ever disagree the reader shows the wrong page while every hash
//! and page count in the sidecar still matches.

use std::path::PathBuf;
use std::sync::Arc;

use crate::{LocalLibrary, PageSource, ServeState, load_works};

/// The page index this test asks `ServeState::page` for, walking `sequenced`.
///
/// # Panics
///
/// Panics if `i` exceeds `u32::MAX`. It cannot: `i` walks `sequenced`, this
/// test's own read of a real corpus volume — a page count, bounded by what a
/// physical book can contain, far below `u32::MAX`.
fn sequenced_index(i: usize) -> u32 {
    u32::try_from(i).expect("sequenced index exceeds u32::MAX")
}

/// How many slots an edition has, for a test that walks every one.
///
/// # Panics
///
/// Panics if `n` exceeds `u32::MAX`. It cannot: `edition.slots(reading).len()`
/// is at most twice the edition's page count (`crate::expand`'s `flat_map` in
/// `taiyaku-core/src/slot.rs` emits at most two `Slot`s per page), and a page
/// count is bounded by what a book can physically contain, far below
/// `u32::MAX`.
fn slot_count(n: usize) -> u32 {
    u32::try_from(n).expect("slot count exceeds u32::MAX")
}

/// The corpus root, or `None` when this machine does not have it.
///
/// **Unset is an absence and skips; set-but-unusable is a mistake and fails.**
/// O30. This one skipped differently from `taiyaku-cli`'s and so failed more
/// quietly: it never looked at the path at all, and each check below then
/// reported `<root>/gotoubun-v01/taiyaku.json absent — skipping`, which names a
/// real path and reads like a corpus missing one volume. The cause was the
/// working directory — cargo gives each package's test binary that package's
/// own directory, so a relative `TAIYAKU_CORPUS=corpus` resolves under
/// `crates/taiyaku-serve/`.
///
/// The `absent` skips below are kept: a corpus that genuinely lacks one volume
/// is an absence, not a mistake. It is only the *root* that must be real.
///
/// Deliberately a second copy rather than shared with `taiyaku-cli`'s: both are
/// `#[cfg(test)]` modules in different crates, and exporting test scaffolding
/// across a crate boundary to save six lines costs more than it saves. The
/// message is pinned by tests there, where nine of the twelve checks live.
fn corpus_root() -> Option<PathBuf> {
    let value = std::env::var("TAIYAKU_CORPUS").ok()?;
    let root = PathBuf::from(&value);
    assert!(
        root.is_dir(),
        "TAIYAKU_CORPUS={value} is not a directory from {}.\n\
         cargo runs each package's tests with that package's own directory as \
         the working directory rather than the workspace root, so a relative \
         path resolves under crates/<package>/ and finds nothing. Pass an \
         absolute path, or run scripts/check-all.sh, which canonicalises it.",
        std::env::current_dir().map_or_else(
            |_| "the working directory".to_string(),
            |d| d.display().to_string()
        )
    );
    Some(root)
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
            let index_u32 = sequenced_index(index);
            let served = state
                .page(work, key, index_u32)
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

/// Note anchors are validated against the *real* editions, not a fixture.
///
/// The fixture `validate` is otherwise tested against is four pages with no
/// spreads, where slots and pages are the same number — which is exactly the
/// case that cannot tell a correct slot check from a page check. Go-Toubun
/// v01's `en-mono` joins seven spreads, so it has seven more slots than pages,
/// and the last seven slots of it are anchors that only a slot-counting check
/// accepts.
///
/// This is the check `docs/status.md` promises for every other subsystem and
/// notes did not have.
#[test]
fn a_note_can_be_anchored_to_every_slot_of_every_real_edition() {
    let Some(root) = corpus_root() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let path = root.join("gotoubun-v01/taiyaku.json");
    if !path.exists() {
        eprintln!("{} absent — skipping", path.display());
        return;
    }

    let works = load_works(std::slice::from_ref(&path)).expect("sidecar loads");
    let work = &works[0];
    let sidecar = work.sidecar();
    let stored = crate::notes::empty(&work.id);

    let note = |edition: &str, slot: u32| crate::notes::Notes {
        work: work.id.clone(),
        notes: vec![crate::notes::Note {
            id: format!("01{edition}{slot}"),
            edition: edition.to_string(),
            slot,
            region: None,
            color: None,
            body: "anchored".into(),
            created: "2026-08-05T10:00:00Z".into(),
            updated: "2026-08-05T10:00:00Z".into(),
        }],
    };

    let mut checked = 0usize;
    let mut spread_editions = 0usize;
    for (key, edition) in &sidecar.editions {
        let slots = slot_count(edition.slots(sidecar.work.reading).len());
        let pages = edition.total_pages();
        if slots > pages {
            spread_editions += 1;
        }
        for slot in 0..slots {
            crate::notes::validate(&note(key, slot), work, &stored)
                .unwrap_or_else(|e| panic!("{key} slot {slot} should anchor a note: {e:?}"));
            checked += 1;
        }
        // And one past the end of each, which is the half that proves the
        // check is not spelled "accept everything".
        assert!(
            crate::notes::validate(&note(key, slots), work, &stored).is_err(),
            "{key} has {slots} slots; slot {slots} must not anchor a note"
        );
    }

    println!(
        "notes: {checked} anchors accepted across {} editions, {spread_editions} of which join \
         spreads and so have more slots than pages",
        sidecar.editions.len()
    );
    assert!(checked > 0, "the corpus sidecar defines no slots at all");
    assert!(
        spread_editions > 0,
        "no edition here joins spreads, so this run could not tell a slot check from a page check"
    );
}

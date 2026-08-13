//! How often a reader meets a gap, and therefore a borrowed page.
//!
//! The design is `docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`.
//! Gated on `TAIYAKU_CORPUS` through `corpus::corpus()` — the *same* gate as
//! `corpus.rs`, deliberately not a second spelling of it, because two
//! spellings is how one ends up skipping in silence (O30).
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli borrow_frequency -- --nocapture
//! ```

use taiyaku_core::{Sidecar, Slot};
use taiyaku_wasm::binding::Binding;

use crate::corpus::corpus;

/// What a reader is shown at one position.
#[derive(Debug, PartialEq, Eq)]
enum Shown {
    /// A real page in the edition being viewed, at this slot of it.
    At(u32),
    /// Another edition's page, and whose.
    Borrowed(String),
    /// No page, and nothing to borrow.
    Gap,
}

/// `web/src/reading.ts`'s `display`, ported.
///
/// Answers the slot in `viewing`, or `None` for a gap. The identity branch
/// matches the original: viewing the authority is always `at`, with no
/// resolver call, because the slot is the authority's own.
fn display(b: &Binding, authority: &str, slot: u32, viewing: &str) -> Option<u32> {
    if viewing == authority {
        return Some(slot);
    }
    let from = b.slot_at(authority, slot)?;
    let landed = b.resolve(authority, viewing, from.page, from.half)?;
    b.index_of(
        viewing,
        Slot {
            page: landed.page,
            half: landed.half,
        },
    )
}

/// `web/src/modules/borrow.ts`'s `lenderFor`, ported.
///
/// `order` is `orderedEditions`: the hub first, then the rest alphabetically.
/// The final `Some(authority)` is the fallback this branch's Task 2 removes;
/// it is here so this measurement reproduces the behaviour that shipped.
fn lender_for(
    b: &Binding,
    order: &[String],
    authority: &str,
    slot: u32,
    viewing: &str,
) -> Option<String> {
    let from = b.slot_at(authority, slot)?;
    for edition in order {
        if edition == viewing || edition == authority {
            continue;
        }
        if let Some(landed) = b.resolve(authority, edition, from.page, from.half) {
            if b.index_of(
                edition,
                Slot {
                    page: landed.page,
                    half: landed.half,
                },
            )
            .is_some()
            {
                return Some(edition.clone());
            }
        }
    }
    Some(authority.to_string())
}

/// `web/src/modules/borrow.ts`'s `withBorrow`, ported.
fn shown_at(b: &Binding, order: &[String], authority: &str, slot: u32, viewing: &str) -> Shown {
    match display(b, authority, slot, viewing) {
        Some(at) => Shown::At(at),
        None => match lender_for(b, order, authority, slot, viewing) {
            Some(lender) => Shown::Borrowed(lender),
            None => Shown::Gap,
        },
    }
}

/// `web/src/session.ts`'s `orderedEditions`: hub first, then the rest sorted.
fn ordered_editions(hub: &str, editions: &[String]) -> Vec<String> {
    let mut rest: Vec<String> = editions.iter().filter(|e| *e != hub).cloned().collect();
    rest.sort();
    let mut order = vec![hub.to_string()];
    order.extend(rest);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two editions where `b` has no counterpart for the hub's slot 2.
    ///
    /// Hand-written JSON rather than a corpus volume, so the port is checked
    /// without the archives and this test runs in the ordinary gate.
    fn two_editions() -> String {
        r#"{
          "taiyaku": 1,
          "work": { "title": "fixture", "reading": "rtl" },
          "hub": "a",
          "editions": {
            "a": { "label": "a", "members": [{ "path": "a", "sha256": "aa", "pages": 4 }] },
            "b": { "label": "b", "members": [{ "path": "b", "sha256": "bb", "pages": 4 }] }
          },
          "alignments": [{
            "from": "a", "to": "b", "by": "solver",
            "solver": { "hash": "dhash-16", "floor": 0.5 },
            "quality": { "verdict": "sound", "slots": 4, "paired": 3, "avg_run": 3.0 },
            "runs": [{ "a": "0-1", "b": "0-1" }, { "a": "2" }, { "a": "3", "b": "2" }]
          }]
        }"#
        .to_string()
    }

    #[test]
    fn ordered_editions_puts_the_hub_first_then_sorts() {
        let editions = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert_eq!(ordered_editions("b", &editions), vec!["b", "a", "c"]);
    }

    #[test]
    fn a_paired_slot_is_at_and_an_unpaired_one_borrows_the_authority() {
        let json = two_editions();
        let b = Binding::from_json(&json).expect("fixture parses");
        let order = ordered_editions("a", &b.editions());
        // Slot 0 pairs, so the reader sees a real `b` page.
        assert_eq!(shown_at(&b, &order, "a", 0, "b"), Shown::At(0));
        // Slot 2 does not, and with only two editions the authority lends —
        // which is exactly the behaviour Task 2 removes.
        assert_eq!(
            shown_at(&b, &order, "a", 2, "b"),
            Shown::Borrowed("a".to_string())
        );
    }
}

/// Every corpus volume that carries a sidecar, swept over every ordered pair.
///
/// Reads `taiyaku.json` where it already exists rather than re-solving: the
/// figures this reproduces were taken against the documents `align --write`
/// produces, and re-solving here would measure the solver as well as the
/// reader. Directories without a sidecar are named as a matter of course,
/// so any skipped volume is visible rather than silent.
#[test]
fn how_often_a_reader_meets_a_gap() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    // Every directory under the root, split on whether it carries a sidecar.
    // The ones without are named rather than dropped: a corpus that had lost
    // one volume's `taiyaku.json` would otherwise sweep eleven volumes and
    // pass, which is the silent skip `corpus.rs`'s header calls out.
    //
    // Dot-prefixed directories are excluded because `.taiyaku-cache` is not a
    // volume and never will be. Nothing else is filtered by name — `incoming/`
    // holds unsorted material and legitimately has no sidecar, so it prints.
    // Naming it here would hide a real volume that happened to be called that,
    // and the line claims only what is true: this directory has no sidecar.
    let (mut volumes, mut without): (Vec<_>, Vec<_>) = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_string_lossy().to_string();
            (path.is_dir() && !name.starts_with('.')).then_some(path)
        })
        .partition(|path| path.join("taiyaku.json").is_file());
    volumes.sort();
    without.sort();
    assert!(
        !volumes.is_empty(),
        "TAIYAKU_CORPUS is set but no volume under it has a taiyaku.json"
    );
    for dir in &without {
        eprintln!(
            "{}: no taiyaku.json, not swept",
            dir.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    let mut total = 0_u32;
    let mut at = 0_u32;
    let mut borrowed = 0_u32;
    let mut gap = 0_u32;
    let mut from_authority = 0_u32;
    let mut from_third = 0_u32;
    let mut runs: Vec<u32> = Vec::new();

    for volume in &volumes {
        let name = volume
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let json = std::fs::read_to_string(volume.join("taiyaku.json")).expect("readable");
        let hub = Sidecar::from_json(&json).expect("parses").hub;
        let b = Binding::from_json(&json).expect("parses");
        let editions = b.editions();
        let order = ordered_editions(&hub, &editions);

        for authority in &editions {
            let Some(count) = b.slot_count(authority) else {
                continue;
            };
            for viewing in &editions {
                if viewing == authority {
                    continue;
                }
                let mut run = 0_u32;
                for slot in 0..count {
                    total += 1;
                    match shown_at(&b, &order, authority, slot, viewing) {
                        Shown::At(_) => {
                            at += 1;
                            if run > 0 {
                                runs.push(run);
                            }
                            run = 0;
                        }
                        Shown::Borrowed(lender) => {
                            borrowed += 1;
                            run += 1;
                            if &lender == authority {
                                from_authority += 1;
                            } else {
                                from_third += 1;
                            }
                        }
                        Shown::Gap => {
                            gap += 1;
                            run += 1;
                        }
                    }
                }
                if run > 0 {
                    runs.push(run);
                }
            }
        }
        eprintln!("{name}: {} editions", editions.len());
    }

    let len1 = runs.iter().filter(|r| **r == 1).count();
    let len4 = runs.iter().filter(|r| **r >= 4).count();
    eprintln!(
        "\npositions {total} | at {at} | borrowed {borrowed} | gap {gap}\n\
         lender: authority {from_authority}, third {from_third}\n\
         runs {} | length 1: {len1} | 4+: {len4} | longest: {}",
        runs.len(),
        runs.iter().max().copied().unwrap_or(0),
    );

    // Structural only. A threshold on a corpus figure is a check that fails
    // when the corpus changes, which is not a measurement — the numbers print
    // for a person to read. What is asserted is that every position landed in
    // exactly one arm, and `borrow.ts`'s own claim that the authority can
    // always lend, which is what Task 2 removes.
    assert_eq!(at + borrowed + gap, total, "every position classified once");
    assert_eq!(gap, 0, "the authority can always lend, before Task 2");
}

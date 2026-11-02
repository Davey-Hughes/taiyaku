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
        if let Some(landed) = b.resolve(authority, edition, from.page, from.half)
            && b.index_of(
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
    None
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
mod tests;

/// Every corpus volume that carries a sidecar, swept over every ordered pair.
///
/// Reads `taiyaku.json` where it already exists rather than re-solving, which
/// would measure the solver as well as the reader. Directories without a
/// sidecar are named as a matter of course, so any skipped volume is visible
/// rather than silent.
#[test]
fn how_often_a_reader_meets_a_gap() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    // Every directory under the root, split on whether it carries a sidecar.
    // The ones without are named rather than dropped: a corpus that had lost
    // one volume's `taiyaku.json` would otherwise sweep the rest and pass,
    // which is the silent skip `corpus.rs`'s header calls out.
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
    // exactly one arm, and that no borrow is ever the authority lending its
    // own page back.
    assert_eq!(at + borrowed + gap, total, "every position classified once");
    assert_eq!(
        from_authority, 0,
        "the authority no longer lends; every borrow is a third edition"
    );
}

//! Which path a lookup walks between two editions, and the answer it gives.
//!
//! `sidecar.rs` owns the document; this owns what can be asked of one. The two
//! were the same file until composition made routing a subject of its own.

use crate::{Run, Sidecar, Slot, counterpart};

/// Which path a lookup walked.
///
/// Reported rather than hidden because a composed answer is weaker than a
/// direct one — measured at about 1.7% of pages lost to hub gaps — and a reader
/// that cannot tell them apart has been told less than the document knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// The same edition on both sides.
    Identity,
    /// An alignment stored in the direction asked for.
    Direct,
    /// An alignment stored the other way round, reversed.
    Inverted,
    /// Two alignments chained through the named hub.
    Hub(String),
}

/// Reverse an alignment: B→A from A→B.
///
/// Paired runs swap sides. Gaps drop, and that is not a loss: a gap records
/// pages of A with no counterpart in B, which says nothing whatever about B.
///
/// This is what lets a sidecar store N-1 edges rather than 2(N-1). It rests on
/// a measurement rather than an assumption — the solver is symmetric on the
/// pairing relation, checked across five pairs in two series including the
/// widest length mismatch in the corpus (195 slots against 240), where every
/// paired run is identical to its transpose. `calibrate` is asymmetric on
/// paper, since `signal` is a median of per-A-page maxima, and it does not
/// reach the DP's output on any material measured. See `notes.md`.
#[must_use]
pub fn invert(runs: &[Run]) -> Vec<Run> {
    let mut inverted: Vec<Run> = runs
        .iter()
        .filter_map(|run| run.b.map(|b| Run::paired(b, run.a)))
        .collect();
    // The solver is monotonic, so swapping a sorted list yields a sorted one —
    // but a hand-edited document need not be monotonic and nothing in the
    // format enforces it. Unsorted, `compress` emits one run per page, which
    // would make a sound composed alignment read as confetti.
    inverted.sort_by_key(|run| run.a.start);
    inverted
}

/// The correspondence between two editions, in slot indices, and how it was
/// found.
///
/// An object rather than a bare function because composing is not free: it
/// reads two alignments, builds two slot tables and chains them. At one page
/// that is nothing; a reader paging through a 195-page volume would pay it 195
/// times. Building it once is the difference between a lookup and a scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub via: Via,
    runs: Vec<Run>,
    from_slots: Vec<Slot>,
    to_slots: Vec<Slot>,
}

impl Route {
    /// Where a slot of the `from` edition lands.
    ///
    /// A reader stopped on the left half of a joined spread has to swap to the
    /// counterpart of *that half*. Resolving its page instead names the first
    /// half, which is a different slot and, where a run ends mid-spread, a
    /// different answer — so a reader doing that moves a page without being
    /// asked to. `notes.md`, "swapping must not round-trip through the
    /// alignment", is the same failure reached by a different route.
    #[must_use]
    pub fn resolve_slot(&self, slot: Slot) -> Option<Slot> {
        let here =
            crate::sidecar::slot_position(crate::sidecar::slot_index(&self.from_slots, slot)?);
        let there = counterpart(&self.runs, here)?;
        self.to_slots.get(there as usize).copied()
    }

    /// Where a page of the `from` edition lands.
    ///
    /// A page reference with no half names the first slot of that page, which
    /// is what makes `47` mean the same thing whether page 47 is whole or a
    /// spread. The answer carries a half because that is what a reader has to
    /// display — possibly one side of a joined spread.
    #[must_use]
    pub fn resolve(&self, page: u32) -> Option<Slot> {
        self.resolve_slot(Slot { page, half: None })
    }

    /// The runs this route walks, in slot indices.
    ///
    /// Exposed because a caller reporting how far the answer can be trusted has
    /// to judge what will actually be walked, which for a composed route is
    /// neither of the stored run lists.
    #[must_use]
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }
}

/// Find the path from one edition to another, or `None` when there is none.
///
/// Order: identity, a stored edge in the direction asked for, the same edge
/// reversed, then two edges chained through the hub.
///
/// **A stored edge is the whole answer, including when it is there and cannot
/// be read.** Falling through to the hub in that case would answer a different
/// question than the one asked — not "where does this edge send the page" but
/// "where does some other path send it". `Sidecar::trust` makes the same
/// choice for the same reason.
#[must_use]
pub fn route(sidecar: &Sidecar, from: &str, to: &str) -> Option<Route> {
    // Existence is checked before identity: `route("nosuch", "nosuch")` must be
    // `None` rather than a confident identity over an edition that is not here.
    let reading = sidecar.work.reading;
    let from_slots = sidecar.editions.get(from)?.slots(reading);
    let to_slots = sidecar.editions.get(to)?.slots(reading);

    if from == to {
        // An edition corresponds to itself as a property of the edition, not of
        // a run list that need not exist — the same rule `trust` applies.
        let runs = match from_slots.len() {
            0 => Vec::new(),
            n => {
                // n > 0 in this arm (0 is handled above), so slot_position(n) - 1
                // cannot underflow.
                let last = crate::sidecar::slot_position(n) - 1;
                vec![Run::paired(
                    crate::PageRange::new(0, last),
                    crate::PageRange::new(0, last),
                )]
            }
        };
        return Some(Route {
            via: Via::Identity,
            runs,
            from_slots,
            to_slots,
        });
    }

    if let Some(edge) = sidecar.edge_between(from, to) {
        let stored = sidecar.runs_of(edge).ok()?;
        let (runs, via) = if edge.from == from {
            (stored, Via::Direct)
        } else {
            (invert(&stored), Via::Inverted)
        };
        return Some(Route {
            via,
            runs,
            from_slots,
            to_slots,
        });
    }

    // No stored edge, so chain two through the hub. This is the topology
    // `sidecar.md` specifies and `topology` measured: 0.00% disagreement across
    // 4,815 page comparisons, about 1.7% of pages lost to hub gaps.
    if !sidecar.editions.contains_key(&sidecar.hub) {
        return None;
    }
    let first = leg(sidecar, from, &sidecar.hub)?;
    let second = leg(sidecar, &sidecar.hub, to)?;
    Some(Route {
        via: Via::Hub(sidecar.hub.clone()),
        runs: crate::compose(&first, &second),
        from_slots,
        to_slots,
    })
}

/// One hop of a composition: a stored edge, oriented the way the caller needs.
///
/// A star stores `hub -> spoke` edges, so the first leg of every spoke-to-spoke
/// lookup is an inversion. That costs nothing the solver would not have
/// produced anyway — see `invert`.
fn leg(sidecar: &Sidecar, from: &str, to: &str) -> Option<Vec<Run>> {
    let edge = sidecar.edge_between(from, to)?;
    let stored = sidecar.runs_of(edge).ok()?;
    Some(if edge.from == from {
        stored
    } else {
        invert(&stored)
    })
}

/// Resolve a page of one edition to its counterpart in another, from a stored
/// sidecar alone.
///
/// This is the whole reader API in one call. A caller resolving more than one
/// page should hold a `Route` instead, which builds the path once.
#[must_use]
pub fn resolve(sidecar: &Sidecar, from: &str, to: &str, page: u32) -> Option<Slot> {
    route(sidecar, from, to)?.resolve(page)
}

#[cfg(test)]
mod tests;

//! Which slots of an edition the alignment never mentions, and where they sit.
//!
//! This is signal 1 of credit-page detection, and the only one that classifies.
//! It is pure — no I/O, no image decoding — so it lives here beside `route` and
//! `slot` rather than in the CLI, and it reports geometry rather than applying
//! a threshold: the command decides what counts.
//!
//! **Runs are split at member boundaries.** Measured on `corpus/gotoubun-v01`'s
//! fan edition, an unsplit run spanning `c005`'s last page and `c005.5`'s first
//! four begins and ends mid-member, so a rule reading position off its endpoints
//! sees no boundary at all — and loses one credits page and four front-matter
//! pages in the same stroke. See `docs/notes.md`, "Corrected 2026-08-05".

use std::collections::BTreeSet;
use std::ops::Range;

use crate::Sidecar;

/// A stretch of one member's pages that the alignment never mentions.
///
/// Both index spaces are carried rather than one, because the `credits` field
/// is in pages and a reader's position is in slots, and a call site converting
/// for itself is how the two silently diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpairedRun {
    /// Half-open, in the edition's flat slot sequence.
    pub slots: Range<u32>,
    /// Half-open, in the edition's flat page sequence.
    pub pages: Range<u32>,
    /// Index into the edition's `members`. One member, always: runs are split
    /// at boundaries so this cannot span two.
    pub member: usize,
    /// Half-open, local to that member.
    pub local: Range<u32>,
    /// Does this run end on the member's last page?
    ///
    /// The only positional test that classifies. Measured four for four on
    /// `corpus/gotoubun-v01`'s fan edition, where the first-index version is
    /// zero for four — what accumulates at a member's start is front matter.
    ///
    /// *Corrected 2026-08-13.* That "zero for four" was gotoubun-v01 alone,
    /// and it does not generalise: `domestic-v01`'s fan edition (wave 2, en-fan
    /// against en-official) is five for five the other way. All five of its
    /// known credit pages sit at a member's *start* — local index 1 in four of
    /// five, local 2 in the first — so the first-index version of this rule
    /// would have caught every one of them there, and the last-index version
    /// this field actually implements catches none. See
    /// `taiyaku-cli/fixtures/credits/domestic-v01-en-fan.json` and
    /// `taiyaku-cli/src/credits.rs`'s
    /// `the_domestic_v01_fixture_is_a_direct_counterexample_to_the_first_index_claim`.
    /// The original claim is left standing above rather than rewritten: it was
    /// sound on the one edition it was measured against, and a doc comment
    /// whose justification changes without a visible trace is the defect this
    /// module's own header exists to avoid. What it no longer supports is
    /// stated as general: a scanlation group's own front matter is *one*
    /// group's habit, not a property of the shape.
    pub at_member_end: bool,
}

/// Which pages of `edition` the alignment against `against` never reaches.
///
/// A page counts as unpaired only when **every** slot it presents is unpaired.
/// A joined spread whose halves are half-reached is content the other edition
/// partly has, and no credits card is half of a spread.
///
/// # Errors
///
/// Returns an error if `edition` names an edition the document does not
/// define, or if no stored alignment joins `edition` and `against` in either
/// direction. It also propagates `Sidecar::runs_of`'s errors for the
/// alignment it finds: `alignment.from` or `alignment.to` naming an edition
/// this document no longer defines, a span whose slot does not exist in
/// either edition, a span with its endpoints reversed, or two paired spans
/// that cover different numbers of slots — all signs of a hand-edited
/// alignment the document can no longer read. The first of those reaches
/// here through `against` rather than `edition`: the `find_map` above
/// matches `against` as a bare string against the stored edge and never
/// checks it against `self.editions`, so an alignment naming a since-deleted
/// edition is still found and handed to `runs_of`, which is where the error
/// actually comes from. This crate has no code path that removes an edition
/// once added — `docs/status.md`'s O24 is the nearer live concern, a direct
/// edge whose far edition survives *under the same name* with replaced
/// content rather than vanishing outright — so this arm is reached only by a
/// document edited by hand. Finally, it refuses if a page already found to
/// be unpaired cannot be located in `edition`'s own member table, which
/// would mean that table and `edition`'s slot table disagree.
///
/// # Panics
///
/// Panics if `slot_start` cannot find a page drawn from `unpaired_pages` in
/// `edition`'s slot table. In practice it cannot: `unpaired_pages` is built
/// by grouping that same `slots` binding's entries by page, and nothing
/// mutates `slots` between the two passes, so every page `slot_start` is
/// asked to find was itself read out of `slots` moments earlier, and its
/// `.position()` search of that identical array always finds it again.
pub fn unpaired_runs(
    sidecar: &Sidecar,
    edition: &str,
    against: &str,
) -> Result<Vec<UnpairedRun>, String> {
    let target = sidecar
        .editions
        .get(edition)
        .ok_or_else(|| format!("this work has no edition `{edition}`"))?;

    // Which side of the stored edge is the edition being examined? The
    // alignment is directed and only one side's spans address this edition's
    // slots, so taking the wrong one silently reports another edition's gaps.
    let (alignment, mine_is_a) = sidecar
        .alignments
        .iter()
        .find_map(|al| match (al.from.as_str(), al.to.as_str()) {
            (f, t) if f == edition && t == against => Some((al, true)),
            (f, t) if f == against && t == edition => Some((al, false)),
            _ => None,
        })
        .ok_or_else(|| format!("no stored alignment joins `{edition}` and `{against}`"))?;

    let runs = sidecar.runs_of(alignment)?;

    let mut mentioned: BTreeSet<u32> = BTreeSet::new();
    for run in &runs {
        // A gap run carries an `a` span and no `b`: those are exactly the
        // pages with no counterpart, so neither side may count them as
        // mentioned.
        let range = if mine_is_a {
            // This side has to ask for it, or it marks the gap as covered and
            // the function answers the opposite of what it is for.
            run.b.map(|_| run.a)
        } else {
            // This side gets that for free — `run.b` is `None`.
            run.b
        };
        if let Some(r) = range {
            mentioned.extend(r.start..=r.end);
        }
    }

    let slots = target.slots(sidecar.work.reading);

    // Group slots by the page they belong to, and keep only pages every one of
    // whose slots is unmentioned.
    let mut unpaired_pages: Vec<u32> = Vec::new();
    let mut at = 0usize;
    while at < slots.len() {
        let page = slots[at].page;
        let first = at;
        while at < slots.len() && slots[at].page == page {
            at += 1;
        }
        if (first..at).all(|i| !mentioned.contains(&crate::sidecar::slot_position(i))) {
            unpaired_pages.push(page);
        }
    }

    // Where each page's slots begin, so a run can report both index spaces.
    let slot_start = |page: u32| -> u32 {
        crate::sidecar::slot_position(
            slots
                .iter()
                .position(|s| s.page == page)
                .expect("every page has at least one slot"),
        )
    };
    let slot_count = |page: u32| -> u32 {
        crate::sidecar::slot_position(slots.iter().filter(|s| s.page == page).count())
    };

    let mut out: Vec<UnpairedRun> = Vec::new();
    for page in unpaired_pages {
        let (member, local) = target
            .locate(page)
            .ok_or_else(|| format!("`{edition}` has no page {page}"))?;

        // Extend the previous run only when this page is its immediate
        // successor *within the same member*. The member test is what splits a
        // run at a boundary, and it is the whole reason this function exists
        // rather than a complement over the slot set.
        match out.last_mut() {
            Some(last) if last.member == member && last.local.end == local => {
                last.pages.end = page + 1;
                last.local.end = local + 1;
                last.slots.end = slot_start(page) + slot_count(page);
            }
            _ => out.push(UnpairedRun {
                slots: slot_start(page)..slot_start(page) + slot_count(page),
                pages: page..page + 1,
                member,
                local: local..local + 1,
                at_member_end: false,
            }),
        }
    }

    for run in &mut out {
        run.at_member_end = run.local.end == target.members[run.member].pages;
    }

    Ok(out)
}

#[cfg(test)]
mod tests;

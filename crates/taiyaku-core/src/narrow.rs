//! Narrowing an edition to the members an account sees.
//!
//! One edition can be merged from files in two libraries' directories, and an
//! account granted only one of them must not be sent the other's pages. It is
//! sent a shorter edition instead, numbered from 0, which the reader reads like
//! any other; everything stored stays in the full numbering, so a page, a slot
//! or a member crosses between the two through here. See spec §3.8 of
//! `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`.

use crate::{Direction, Edition, Half, Slot};

/// An edition as an account that sees only some of its members sees it (spec §3.8).
#[derive(Debug)]
pub struct Narrowed {
    edition: Edition,
    /// Kept whole so its slots are numbered by `Edition::slots`, the numbering
    /// stored positions and notes use.
    full: Edition,
    /// The full index of each narrowed page, ascending.
    pages: Vec<u32>,
    /// The full index of each narrowed member.
    members: Vec<usize>,
}

/// Where a full slot lands in a narrowed edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromFull {
    /// The slot's index in the narrowed edition.
    Visible(u32),
    /// The slot's page is hidden. `fallback` is the first slot of the nearest
    /// visible page before it, or slot 0 when no visible page comes before it.
    Hidden { fallback: u32 },
    /// Past the end of the full edition.
    Outside,
}

impl Narrowed {
    /// `visible[i]` says whether `edition.members[i]` is visible. `None` when
    /// `visible.len() != edition.members.len()`, or when no visible member has a page.
    #[must_use]
    pub fn new(edition: &Edition, visible: &[bool]) -> Option<Narrowed> {
        if visible.len() != edition.members.len() {
            return None;
        }
        let mut members = Vec::new();
        let mut pages = Vec::new();
        let mut start = 0u32;
        for (index, (member, &seen)) in edition.members.iter().zip(visible).enumerate() {
            if seen {
                members.push(index);
                pages.extend(start..start + member.pages);
            }
            start += member.pages;
        }
        if pages.is_empty() {
            return None;
        }
        // A spread is one wide page, so keeping it with its page keeps both
        // halves or neither.
        let renumber = |list: &[u32]| -> Vec<u32> {
            list.iter()
                .filter_map(|&page| narrowed_page(&pages, page))
                .collect()
        };
        let narrowed = Edition {
            label: edition.label.clone(),
            members: members
                .iter()
                .map(|&index| edition.members[index].clone())
                .collect(),
            spreads: renumber(&edition.spreads),
            credits: renumber(&edition.credits),
        };
        Some(Narrowed {
            edition: narrowed,
            full: edition.clone(),
            pages,
            members,
        })
    }

    /// The edition the account is sent: the visible members in order, spreads
    /// and credits kept where their page is visible, renumbered.
    #[must_use]
    pub fn edition(&self) -> &Edition {
        &self.edition
    }

    /// Narrowed page index to full; `None` past the end.
    #[must_use]
    pub fn page_to_full(&self, page: u32) -> Option<u32> {
        self.pages.get(page as usize).copied()
    }

    /// Full page index to narrowed; `None` for a hidden page or past the end.
    #[must_use]
    pub fn page_from_full(&self, page: u32) -> Option<u32> {
        narrowed_page(&self.pages, page)
    }

    /// Narrowed member index to its index among the full edition's members.
    #[must_use]
    pub fn member_to_full(&self, member: usize) -> Option<usize> {
        self.members.get(member).copied()
    }

    /// Narrowed slot to full, through the slot's page and half; `None` past the end.
    #[must_use]
    pub fn slot_to_full(&self, slot: u32, reading: Direction) -> Option<u32> {
        let at = *self.edition.slots(reading).get(slot as usize)?;
        let page = self.page_to_full(at.page)?;
        slot_index(&self.full.slots(reading), page, at.half)
    }

    /// Full slot to narrowed.
    #[must_use]
    pub fn slot_from_full(&self, slot: u32, reading: Direction) -> FromFull {
        let Some(&at) = self.full.slots(reading).get(slot as usize) else {
            return FromFull::Outside;
        };
        let slots = self.edition.slots(reading);
        if let Some(page) = self.page_from_full(at.page) {
            // A visible page keeps its spread flag, so its half is in the
            // narrowed slots as well. A miss is that invariant broken, which
            // would otherwise come back quietly as a hidden page.
            let index = slot_index(&slots, page, at.half);
            debug_assert!(
                index.is_some(),
                "full slot {slot} is on narrowed page {page}, but its half is not a narrowed slot"
            );
            if let Some(index) = index {
                return FromFull::Visible(index);
            }
        }
        let before = self.pages.partition_point(|&page| page < at.page);
        let fallback = before
            .checked_sub(1)
            .and_then(|page| u32::try_from(page).ok())
            .and_then(|page| first_slot(&slots, |s| s.page == page))
            .unwrap_or(0);
        FromFull::Hidden { fallback }
    }
}

fn narrowed_page(pages: &[u32], page: u32) -> Option<u32> {
    let index = pages.binary_search(&page).ok()?;
    u32::try_from(index).ok()
}

fn slot_index(slots: &[Slot], page: u32, half: Option<Half>) -> Option<u32> {
    first_slot(slots, |s| s.page == page && s.half == half)
}

fn first_slot(slots: &[Slot], matches: impl Fn(&Slot) -> bool) -> Option<u32> {
    let index = slots.iter().position(matches)?;
    u32::try_from(index).ok()
}

#[cfg(test)]
mod tests;

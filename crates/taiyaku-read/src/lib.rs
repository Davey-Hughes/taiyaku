//! Reading an edition's pages from the containers it is stored in.
//!
//! Split out of `taiyaku-cli` when `taiyaku-serve` became a second consumer.
//! The page sequence a member contributes is normative — `docs/sidecar.md`,
//! "How a member becomes pages" — so it has to be derived in exactly one
//! place. Two callers deriving it separately is the defect this crate exists
//! to make impossible.

mod naming;
mod pages;

pub use naming::natural_cmp;
pub use pages::{
    Page, member_digest, member_page, member_page_names, member_pages, read_edition,
    read_members_in_order,
};

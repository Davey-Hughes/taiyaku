//! Reading an edition's pages from the containers it is stored in.
//!
//! The page sequence a member contributes is normative — `docs/sidecar.md`,
//! "How a member becomes pages" — so it has to be derived in exactly one
//! place. Two callers deriving it separately is the defect this crate exists
//! to make impossible.
//!
//! It also reads a book's text: [`read_book`] turns an EPUB into an ordered
//! list of text blocks, for the books work.

pub mod atomic;
mod book;
mod format;
mod naming;
mod pages;
#[cfg(rar)]
mod rar;

pub use book::{
    Block, BlockId, BlockKind, Book, Figure, ImageLink, LinkTarget, Mark, MarkKind, read_book,
};
pub use format::{Format, sniff};
pub use naming::{chapter_number, natural_cmp, volume_number, volume_number_marked};
pub use pages::{
    Page, PageEntry, archive_page, for_each_member_page, is_page_name, member_digest, member_entry,
    member_page, member_page_entries, member_page_names, member_page_sizes, member_pages,
    member_unread_entries, read_edition, read_members_in_order,
};

/// How much a single image decode may allocate.
///
/// O10: here, rather than in either consumer. A copy in each of
/// `taiyaku-cli`'s fingerprinting and `taiyaku-serve`'s cover generation,
/// across a crate boundary with no dependency edge between the two, would each
/// need a comment asking whoever changed one to keep the other in step. That
/// is the arrangement that drifts. Both depend on this crate — it is the one
/// they share — and both decode pages *this* crate handed them, so the
/// ceiling on that decode belongs alongside the bytes.
///
/// Not `taiyaku-core`: that crate compiles to WebAssembly and describes the
/// format. A native decoder's memory ceiling is neither.
///
/// The `image` default is 512 MB, which rejects this corpus's largest pages
/// outright — the Color Walk art books run 430 MB at 10200×14039, and the
/// buffer plus the decoder's working space crosses it. Raised rather than
/// removed, because the limit is what turns a malformed header claiming
/// enormous dimensions into an error instead of an out-of-memory kill.
///
/// This bounds how large one decode may get, not how many run at once. A
/// cover request is more exposed than the aligner on the second count — it
/// runs inline in a request handler rather than a batch job — which is why
/// `taiyaku-serve` bounds its concurrency separately.
pub const MAX_DECODE_ALLOC: u64 = 3 * 1024 * 1024 * 1024;

#[cfg(test)]
mod corpus;
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod test_support;

#[cfg(test)]
mod tests;

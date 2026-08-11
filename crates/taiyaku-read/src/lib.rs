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
    Page, is_page_name, member_digest, member_page, member_page_names, member_page_sizes,
    member_pages, read_edition, read_members_in_order,
};

/// How much a single image decode may allocate.
///
/// O10: here, rather than in either consumer. `taiyaku-cli`'s fingerprinting
/// and `taiyaku-serve`'s cover generation each carried their own copy, across
/// a crate boundary with no dependency edge between the two, each with a
/// comment asking whoever changed one to keep the other in step. That is the
/// arrangement that drifts. Both already depend on this crate — it is the one
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
mod tests {
    use super::*;

    /// The ceiling is a number, and nothing asserted it — so every arithmetic
    /// mutation of `3 * 1024 * 1024 * 1024` survived, including ones that lower
    /// it below the corpus's largest page.
    ///
    /// Written as the two facts the doc comment above claims rather than as a
    /// restatement of the expression, because `assert_eq!(X, 3 * 1024 * 1024 *
    /// 1024)` is the expression again and would move with any edit to it. What
    /// the constant has to be true of: it must admit the largest page in the
    /// corpus, and the `image` default it replaces must not.
    #[test]
    fn the_decode_ceiling_admits_the_largest_page_in_the_corpus() {
        // The Color Walk art books, at 10200×14039 — the largest pages the
        // corpus holds, and the reason this constant exists. `image` bounds the
        // decoded buffer, which is four bytes a pixel.
        const WIDEST: u64 = 10_200;
        const TALLEST: u64 = 14_039;
        const IMAGE_DEFAULT: u64 = 512 * 1024 * 1024;

        assert_eq!(MAX_DECODE_ALLOC, 3_221_225_472, "three GiB");

        let decoded = WIDEST * TALLEST * 4;
        assert!(
            MAX_DECODE_ALLOC > decoded,
            "must admit a {decoded}-byte decode; the ceiling is {MAX_DECODE_ALLOC}"
        );

        assert!(
            IMAGE_DEFAULT < decoded,
            "and the default this replaces must not, or raising it bought nothing"
        );
    }
}
